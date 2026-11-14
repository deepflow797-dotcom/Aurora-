package app.aurora.player

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Pause
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material3.Button
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Slider
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel

/**
 * One screen: folder, library, transport.
 *
 * Everything visual is a pure function of [UiState]; the only side effects are
 * calls into [PlayerViewModel]. That split is why the podcast/demo screens the
 * desktop build will grow can reuse the same model.
 */
class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        setContent {
            AuroraTheme {
                val model: PlayerViewModel = viewModel()
                val ui by model.ui.collectAsStateWithLifecycle()
                AuroraScreen(
                    ui = ui,
                    onFolder = model::setFolder,
                    onScan = model::scan,
                    onOpen = model::open,
                    onToggle = model::toggle,
                    onSeek = model::seek,
                    onVolume = model::setVolume,
                )
            }
        }
    }
}

@Composable
fun AuroraTheme(content: @Composable () -> Unit) {
    // Dark by default: this is a music player, and the spec's shell is dark.
    MaterialTheme(
        colorScheme = darkColorScheme(
            primary = Color(0xFF7FB2FF),
            background = Color(0xFF0E0F12),
            surface = Color(0xFF16181D),
        ),
        content = content,
    )
}

@Composable
fun AuroraScreen(
    ui: UiState,
    onFolder: (String) -> Unit,
    onScan: () -> Unit,
    onOpen: (LibraryTrack) -> Unit,
    onToggle: () -> Unit,
    onSeek: (Long) -> Unit,
    onVolume: (Float) -> Unit,
) {
    val snapshot = ui.snapshot
    Surface(modifier = Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.background) {
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(horizontal = 20.dp, vertical = 48.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Text(
                text = stringResource(R.string.app_name),
                style = MaterialTheme.typography.headlineMedium,
                color = MaterialTheme.colorScheme.onBackground,
            )

            Row(verticalAlignment = Alignment.CenterVertically) {
                OutlinedTextField(
                    value = ui.folder,
                    onValueChange = onFolder,
                    modifier = Modifier.weight(1f),
                    singleLine = true,
                    label = { Text(stringResource(R.string.library_hint)) },
                )
                Spacer(Modifier.padding(4.dp))
                Button(onClick = onScan, enabled = !ui.busy) {
                    Text(stringResource(R.string.scan))
                }
            }

            // Now playing.
            Column(Modifier.fillMaxWidth()) {
                Text(
                    text = snapshot?.title ?: stringResource(R.string.nothing_loaded),
                    style = MaterialTheme.typography.titleLarge,
                    color = MaterialTheme.colorScheme.onBackground,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                Text(
                    text = snapshot?.subtitle.orEmpty(),
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                snapshot?.error?.let { message ->
                    Text(
                        text = message,
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.error,
                    )
                }
            }

            var dragging by remember { mutableStateOf<Float?>(null) }
            val duration = (snapshot?.durationMs ?: 0L).coerceAtLeast(1L)
            val position = dragging?.toLong() ?: (snapshot?.positionMs ?: 0L)
            Slider(
                value = position.coerceIn(0L, duration).toFloat(),
                onValueChange = { dragging = it },
                onValueChangeFinished = {
                    dragging?.let { onSeek(it.toLong()) }
                    dragging = null
                },
                valueRange = 0f..duration.toFloat(),
            )
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
            ) {
                Text(text = format(position), color = MaterialTheme.colorScheme.onSurfaceVariant)
                Text(
                    text = snapshot?.durationMs?.let(::format) ?: "--:--",
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }

            Row(
                modifier = Modifier.fillMaxWidth(),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                IconButton(onClick = onToggle) {
                    Icon(
                        imageVector = if (snapshot?.isPlaying == true) Icons.Filled.Pause else Icons.Filled.PlayArrow,
                        contentDescription = stringResource(
                            if (snapshot?.isPlaying == true) R.string.pause else R.string.play
                        ),
                        tint = MaterialTheme.colorScheme.primary,
                    )
                }
                Slider(
                    value = snapshot?.volume ?: 1f,
                    onValueChange = onVolume,
                    modifier = Modifier.weight(1f),
                )
            }

            // Say it on screen: the timeline is real, the sound is not.
            Text(
                text = snapshot?.output ?: stringResource(R.string.simulated_output),
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier
                    .fillMaxWidth()
                    .background(MaterialTheme.colorScheme.surface)
                    .padding(8.dp),
            )

            Spacer(Modifier.height(4.dp))

            LazyColumn(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                items(ui.library) { track ->
                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
                            .padding(vertical = 6.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Column(Modifier.weight(1f)) {
                            Text(
                                text = track.title,
                                color = MaterialTheme.colorScheme.onBackground,
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis,
                            )
                            Text(
                                text = "${track.artist} · ${track.codec ?: "?"} · ${format(track.durationMs)}",
                                style = MaterialTheme.typography.bodySmall,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis,
                            )
                        }
                        Button(onClick = { onOpen(track) }) {
                            Text(stringResource(R.string.play))
                        }
                    }
                }
            }
        }
    }
}

private fun format(ms: Long): String {
    val totalSeconds = ms / 1000
    return "%d:%02d".format(totalSeconds / 60, totalSeconds % 60)
}
