package app.aurora.player

import org.json.JSONArray
import org.json.JSONObject

/**
 * The four JNI entry points exported by `aurora-mobile`.
 *
 * Nothing here decides anything: each function hands a string to Rust and gets a
 * JSON string back. The logic - playback, scanning, the clock - lives in
 * `crates/aurora-mobile/src/facade.rs`, where it is unit-tested on a normal
 * machine; this file exists only so Kotlin can reach it.
 *
 * Symbol names are `Java_app_aurora_player_NativePlayer_<name>`; renaming the
 * package or this object breaks the link at run time, which is why
 * `jni_bridge.rs` asserts the mangling in a test.
 */
object NativePlayer {
    init {
        System.loadLibrary("aurora_mobile")
    }

    external fun nativeCreate(rootsJson: String): Long
    external fun nativeCall(handle: Long, method: String, argsJson: String): String
    external fun nativeDestroy(handle: Long): Boolean
    external fun nativeLivePlayers(): Long
}

/** One track the scanner found. */
data class LibraryTrack(
    val path: String,
    val title: String,
    val artist: String,
    val codec: String?,
    val durationMs: Long,
)

/** Everything the screen renders, parsed from one `state` call. */
data class Snapshot(
    val state: String,
    val positionMs: Long,
    val durationMs: Long?,
    val volume: Float,
    val output: String,
    val title: String?,
    val subtitle: String?,
    val error: String?,
) {
    val isPlaying: Boolean get() = state == "playing"

    companion object {
        fun from(json: JSONObject): Snapshot {
            val track = json.optJSONObject("track")
            val artist = track?.optString("artist").orEmpty()
            val rate = track?.optInt("sampleRateHz", 0) ?: 0
            val channels = track?.optInt("channels", 0) ?: 0
            return Snapshot(
                state = json.optString("state", "idle"),
                positionMs = json.optLong("positionMs", 0),
                durationMs = if (json.isNull("durationMs")) null else json.optLong("durationMs"),
                volume = json.optDouble("volume", 1.0).toFloat(),
                output = json.optString("output", ""),
                title = track?.optString("title"),
                subtitle = if (track == null) null else "$artist · ${rate / 1000} kHz · ${channels}ch",
                error = if (json.isNull("error")) null else json.optString("error"),
            )
        }

        fun error(message: String) = Snapshot(
            state = "error",
            positionMs = 0,
            durationMs = null,
            volume = 1f,
            output = "",
            title = null,
            subtitle = null,
            error = message,
        )
    }
}

/**
 * A handle plus the JSON plumbing.
 *
 * Deliberately dumb: every call crosses JNI once and returns the whole state, so
 * the UI can never hold a half-updated model. That is cheap here (a few hundred
 * bytes of JSON per poll) and removes a whole class of desynchronisation bugs.
 */
class Player(configRoot: String, dataRoot: String, cacheRoot: String) : AutoCloseable {
    private var handle: Long = NativePlayer.nativeCreate(
        JSONObject()
            .put("configRoot", configRoot)
            .put("dataRoot", dataRoot)
            .put("cacheRoot", cacheRoot)
            .toString()
    )

    val isOpen: Boolean get() = handle != 0L

    private fun call(method: String, args: JSONObject = JSONObject()): JSONObject {
        if (handle == 0L) {
            return JSONObject().put("error", "no_such_player").put("message", "player is closed")
        }
        val response = NativePlayer.nativeCall(handle, method, args.toString())
        return JSONObject(response)
    }

    fun state(): Snapshot {
        val json = call("state")
        return if (json.has("error")) Snapshot.error(json.optString("message")) else Snapshot.from(json)
    }

    fun open(path: String): Snapshot {
        val json = call("open", JSONObject().put("path", path))
        return if (json.has("error")) Snapshot.error(json.optString("message")) else state()
    }

    fun play(): Snapshot = toggle("play")
    fun pause(): Snapshot = toggle("pause")
    fun toggle(): Snapshot = toggle("toggle")

    private fun toggle(method: String): Snapshot {
        val json = call(method)
        return if (json.has("error")) Snapshot.error(json.optString("message")) else Snapshot.from(json)
    }

    fun seek(positionMs: Long): Snapshot {
        call("seek", JSONObject().put("positionMs", positionMs))
        return state()
    }

    fun setVolume(volume: Float): Snapshot {
        call("setVolume", JSONObject().put("volume", volume.toDouble()))
        return state()
    }

    fun scan(folder: String): List<LibraryTrack> {
        val json = call("scan", JSONObject().put("folder", folder))
        val array: JSONArray = json.optJSONArray("library") ?: JSONArray()
        return (0 until array.length()).map { index ->
            val track = array.getJSONObject(index)
            LibraryTrack(
                path = track.optString("path"),
                title = track.optString("title"),
                artist = track.optString("artist"),
                codec = if (track.isNull("codec")) null else track.optString("codec"),
                durationMs = track.optLong("durationMs"),
            )
        }
    }

    override fun close() {
        if (handle != 0L) {
            NativePlayer.nativeDestroy(handle)
            handle = 0L
        }
    }
}
