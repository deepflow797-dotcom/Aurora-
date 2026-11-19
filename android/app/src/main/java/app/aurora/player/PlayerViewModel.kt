package app.aurora.player

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/** What the screen draws. One immutable value, replaced on every poll. */
data class UiState(
    val folder: String = "",
    val library: List<LibraryTrack> = emptyList(),
    val snapshot: Snapshot? = null,
    val lyrics: Lyrics? = null,
    val busy: Boolean = false,
)

/**
 * The only place that talks to Rust.
 *
 * Two rules, both learned from the desktop build:
 *
 * * the UI never advances a position - it polls the snapshot and the native clock
 *   does the interpolating, so a late frame or a paused window cannot drift;
 * * every JNI call runs on [Dispatchers.IO], never on the main thread, because a
 *   `scan` of a real music folder takes seconds and Android will not forgive that.
 */
class PlayerViewModel(application: Application) : AndroidViewModel(application) {
    private val player: Player
    private val _ui = MutableStateFlow(UiState())
    val ui: StateFlow<UiState> = _ui.asStateFlow()
    private var pollJob: Job? = null

    init {
        val files = application.filesDir
        player = Player(
            configRoot = files.resolve("config").absolutePath,
            dataRoot = files.resolve("data").absolutePath,
            cacheRoot = application.cacheDir.resolve("aurora").absolutePath,
        )
        // The player may have failed to start (unusable roots); say so on screen
        // instead of crashing in the constructor.
        if (!player.isOpen) {
            _ui.value = _ui.value.copy(snapshot = Snapshot.error("native player could not start"))
        } else {
            refresh()
        }
    }

    fun setFolder(folder: String) {
        _ui.value = _ui.value.copy(folder = folder)
    }

    fun scan() {
        val folder = _ui.value.folder.ifBlank {
            getApplication<Application>().getExternalFilesDir(null)?.absolutePath.orEmpty()
        }
        if (folder.isBlank()) return
        _ui.value = _ui.value.copy(busy = true, folder = folder)
        viewModelScope.launch {
            val tracks = withContext(Dispatchers.IO) { player.scan(folder) }
            _ui.value = _ui.value.copy(library = tracks, busy = false)
        }
    }

    fun open(track: LibraryTrack) {
        viewModelScope.launch {
            // `open` also loads the sidecar lyrics, so the first poll already has a
            // column to draw.
            val snapshot = withContext(Dispatchers.IO) { player.open(track.path) }
            val lyrics = withContext(Dispatchers.IO) { player.lyrics() }
            _ui.value = _ui.value.copy(snapshot = snapshot, lyrics = lyrics)
            startPolling()
        }
    }

    fun toggle() {
        viewModelScope.launch {
            val snapshot = withContext(Dispatchers.IO) { player.toggle() }
            _ui.value = _ui.value.copy(snapshot = snapshot)
            if (snapshot.isPlaying) startPolling() else stopPolling()
        }
    }

    fun seek(positionMs: Long) {
        viewModelScope.launch {
            val snapshot = withContext(Dispatchers.IO) { player.seek(positionMs) }
            _ui.value = _ui.value.copy(snapshot = snapshot)
        }
    }

    fun setVolume(volume: Float) {
        viewModelScope.launch {
            val snapshot = withContext(Dispatchers.IO) { player.setVolume(volume) }
            _ui.value = _ui.value.copy(snapshot = snapshot)
        }
    }

    /** 200 ms is enough for a progress bar because the clock extrapolates. */
    private fun startPolling() {
        if (pollJob?.isActive == true) return
        pollJob = viewModelScope.launch {
            while (isActive) {
                val snapshot = withContext(Dispatchers.IO) { player.state() }
                // One extra call per tick: the lyric column needs the plan at the
                // same instant the position is read, and both come from the same
                // native clock.
                val lyrics = withContext(Dispatchers.IO) { player.lyrics() }
                _ui.value = _ui.value.copy(snapshot = snapshot, lyrics = lyrics)
                if (!snapshot.isPlaying && snapshot.state != "playing") stopPolling()
                delay(200)
            }
        }
    }

    private fun stopPolling() {
        pollJob?.cancel()
        pollJob = null
    }

    private fun refresh() {
        viewModelScope.launch {
            val snapshot = withContext(Dispatchers.IO) { player.state() }
            _ui.value = _ui.value.copy(snapshot = snapshot)
        }
    }

    override fun onCleared() {
        stopPolling()
        // Closing here is what keeps `live_players` at zero: the JNI handle is the
        // only owner of the native player.
        player.close()
        super.onCleared()
    }
}
