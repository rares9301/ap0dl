package com.ap0dl.anyomi.ui.screens

import android.text.format.Formatter
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Close
import androidx.compose.material.icons.rounded.DeleteOutline
import androidx.compose.material.icons.rounded.Download
import androidx.compose.material.icons.rounded.PlayArrow
import androidx.compose.material.icons.rounded.Refresh
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.ap0dl.anyomi.AppViewModel
import com.ap0dl.anyomi.data.DownloadStatus
import com.ap0dl.anyomi.data.Loadable
import com.ap0dl.anyomi.data.LocalDownload
import com.ap0dl.anyomi.data.displayTitle
import com.ap0dl.anyomi.ui.components.CoverImage
import com.ap0dl.anyomi.ui.components.EmptyPanel
import com.ap0dl.anyomi.ui.components.PrimaryAction
import com.ap0dl.anyomi.ui.components.SectionHeading
import com.ap0dl.anyomi.ui.components.SourcePill
import com.ap0dl.anyomi.ui.theme.Amber
import com.ap0dl.anyomi.ui.theme.Ice
import com.ap0dl.anyomi.ui.theme.Mint
import com.ap0dl.anyomi.ui.theme.Muted
import com.ap0dl.anyomi.ui.theme.Night
import com.ap0dl.anyomi.ui.theme.NightCard
import com.ap0dl.anyomi.ui.theme.NightLine
import com.ap0dl.anyomi.ui.theme.Paper
import kotlinx.coroutines.delay

@Composable
fun DownloadsScreen(
    app: AppViewModel,
    onBrowse: () -> Unit,
    onPlayLocal: (LocalDownload) -> Unit,
) {
    val active by app.activeDownloads.collectAsStateWithLifecycle()
    val local by app.localDownloads.collectAsStateWithLifecycle()
    val notice by app.downloadNotice.collectAsStateWithLifecycle()
    val context = LocalContext.current
    var deleteTarget by remember { mutableStateOf<LocalDownload?>(null) }
    val totalBytes = local.sumOf { it.size }

    LaunchedEffect(notice) {
        if (notice != null) {
            delay(3_500)
            app.clearDownloadNotice()
        }
    }

    LazyColumn(
        modifier = Modifier.fillMaxWidth().background(Night).statusBarsPadding(),
        contentPadding = PaddingValues(bottom = 28.dp),
    ) {
        item {
            Row(
                modifier = Modifier.fillMaxWidth().padding(horizontal = 20.dp, vertical = 16.dp),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Column {
                    Text(
                        "DOWNLOADS",
                        color = Mint,
                        style = MaterialTheme.typography.labelMedium,
                        letterSpacing = 2.sp,
                    )
                    Text(
                        "Offline, no signal.",
                        color = Paper,
                        style = MaterialTheme.typography.headlineLarge,
                    )
                }
                Surface(
                    color = Mint.copy(alpha = 0.1f),
                    shape = RoundedCornerShape(50),
                ) {
                    Row(
                        Modifier.padding(horizontal = 10.dp, vertical = 6.dp),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(5.dp),
                    ) {
                        Icon(
                            Icons.Rounded.Download,
                            contentDescription = null,
                            tint = Mint,
                            modifier = Modifier.size(14.dp),
                        )
                        Text(
                            "${local.size} SAVED",
                            color = Mint,
                            style = MaterialTheme.typography.labelSmall,
                        )
                    }
                }
            }
        }
        item {
            AnimatedVisibility(
                visible = notice != null,
                enter = fadeIn(),
                exit = fadeOut(),
            ) {
                Surface(
                    modifier = Modifier.fillMaxWidth().padding(horizontal = 20.dp, vertical = 4.dp),
                    color = Ice.copy(alpha = 0.1f),
                    shape = RoundedCornerShape(13.dp),
                    border = BorderStroke(1.dp, Ice.copy(alpha = 0.2f)),
                ) {
                    Text(
                        notice.orEmpty(),
                        modifier = Modifier.padding(horizontal = 12.dp, vertical = 10.dp),
                        color = Ice,
                        style = MaterialTheme.typography.bodySmall,
                    )
                }
            }
        }
        if (active.isNotEmpty()) {
            item {
                SectionHeading(
                    title = "In progress",
                    eyebrow = "Native Rust downloader",
                    modifier = Modifier.padding(horizontal = 20.dp).padding(top = 12.dp),
                )
                Spacer(Modifier.height(8.dp))
            }
            items(active, key = { "active:${it.id}" }) { status ->
                ActiveDownloadCard(status = status, onDismiss = { app.dismissDownload(status) })
            }
        }
        item {
            SectionHeading(
                title = "Ready offline",
                eyebrow = if (local.isEmpty()) "Your saved episodes" else "${local.size} episodes · ${Formatter.formatShortFileSize(context, totalBytes)}",
                action = if (local.isEmpty()) null else "Refresh",
                onAction = { app.refreshDownloads() },
                modifier = Modifier.padding(horizontal = 20.dp).padding(top = 20.dp),
            )
            Spacer(Modifier.height(8.dp))
        }
        if (local.isEmpty()) {
            item {
                Column(Modifier.padding(horizontal = 20.dp)) {
                    EmptyPanel(
                        title = "No downloads yet",
                        message = "Open any episode list and tap the download icon to keep it on this device for offline playback.",
                    )
                    PrimaryAction(
                        text = "Browse anime",
                        modifier = Modifier.fillMaxWidth(),
                        onClick = onBrowse,
                    )
                }
            }
        } else {
            items(local, key = { "local:${it.path}" }) { item ->
                LocalDownloadCard(
                    item = item,
                    onPlay = { onPlayLocal(item) },
                    onDelete = { deleteTarget = item },
                )
            }
        }
        item {
            Spacer(Modifier.height(16.dp))
            Text(
                "Files are stored privately in Anyomi's app storage. HLS episodes are saved as MPEG-TS because Android does not bundle ffmpeg for remuxing.",
                modifier = Modifier.padding(horizontal = 20.dp),
                color = Muted.copy(alpha = 0.65f),
                style = MaterialTheme.typography.bodySmall,
            )
        }
    }

    deleteTarget?.let { target ->
        AlertDialog(
            onDismissRequest = { deleteTarget = null },
            title = { Text("Delete download?") },
            text = { Text("${target.title} · ${target.episodeLabel} will be removed from this device.") },
            confirmButton = {
                TextButton(
                    onClick = {
                        app.deleteDownload(target)
                        deleteTarget = null
                    },
                ) { Text("Delete", color = MaterialTheme.colorScheme.error) }
            },
            dismissButton = {
                TextButton(onClick = { deleteTarget = null }) { Text("Cancel") }
            },
            containerColor = NightCard,
        )
    }
}

@Composable
private fun ActiveDownloadCard(status: DownloadStatus, onDismiss: () -> Unit) {
    Surface(
        modifier = Modifier.fillMaxWidth().padding(horizontal = 20.dp, vertical = 4.dp),
        color = NightCard.copy(alpha = 0.78f),
        shape = RoundedCornerShape(18.dp),
        border = BorderStroke(1.dp, NightLine.copy(alpha = 0.65f)),
    ) {
        Row(Modifier.padding(12.dp), horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            Box(
                Modifier
                    .width(54.dp)
                    .aspectRatio(2f / 3f)
                    .clip(RoundedCornerShape(11.dp)),
            ) {
                CoverImage(
                    url = status.cover,
                    title = status.animeTitle,
                    modifier = Modifier.fillMaxWidth().height(81.dp),
                )
            }
            Column(Modifier.weight(1f)) {
                Text(
                    status.animeTitle,
                    color = Paper,
                    style = MaterialTheme.typography.titleMedium,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                val titleSuffix = status.episodeTitle
                    ?.takeUnless { it.startsWith("Episode", ignoreCase = true) }
                    ?.let { " · $it" }
                    .orEmpty()
                Text(
                    "Episode ${status.episodeNumber.toInt()}$titleSuffix${
                        status.audio?.let { " · ${it.replace("Sub & Dub", "Dual audio")}" } ?: ""
                    }",
                    color = if (status.audioFallback) Amber else Muted,
                    style = MaterialTheme.typography.bodySmall,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                Spacer(Modifier.height(6.dp))
                SourcePill(providerId = status.provider, compact = true)
                Spacer(Modifier.height(9.dp))
                val progress = status.progress
                if (status.status == "failed") {
                    Text(
                        status.error ?: "Download failed",
                        color = MaterialTheme.colorScheme.error,
                        style = MaterialTheme.typography.bodySmall,
                        maxLines = 2,
                        overflow = TextOverflow.Ellipsis,
                    )
                } else {
                    if (progress != null) {
                        LinearProgressIndicator(
                            progress = { progress },
                            modifier = Modifier.fillMaxWidth().height(4.dp).clip(CircleShape),
                            color = Mint,
                            trackColor = Color.White.copy(alpha = 0.12f),
                            drawStopIndicator = {},
                        )
                        Spacer(Modifier.height(5.dp))
                    } else {
                        LinearProgressIndicator(
                            modifier = Modifier.fillMaxWidth().height(4.dp).clip(CircleShape),
                            color = Mint,
                            trackColor = Color.White.copy(alpha = 0.12f),
                        )
                        Spacer(Modifier.height(5.dp))
                    }
                    Text(
                        downloadStatusLine(status, progress),
                        color = Muted,
                        style = MaterialTheme.typography.labelSmall,
                    )
                }
            }
            IconButton(onClick = onDismiss, modifier = Modifier.size(30.dp)) {
                Icon(
                    Icons.Rounded.Close,
                    contentDescription = "Dismiss",
                    tint = Muted,
                    modifier = Modifier.size(18.dp),
                )
            }
        }
    }
}

private fun downloadStatusLine(status: DownloadStatus, progress: Float?): String {
    val percent = progress?.let { " · ${(it * 100).toInt()}%" }.orEmpty()
    return when (status.status) {
        "queued" -> "Queued"
        "resolving" -> "Resolving sources…"
        "downloading" -> {
            if (status.segmentsTotal != null) {
                "HLS segment ${status.segmentsDone ?: 0} of ${status.segmentsTotal}$percent"
            } else if (status.totalBytes != null) {
                "${formatBytes(status.downloadedBytes)} of ${formatBytes(status.totalBytes)}$percent"
            } else {
                "Downloading · ${formatBytes(status.downloadedBytes)}$percent"
            }
        }
        "completed" -> "Completed"
        "failed" -> "Failed"
        else -> status.status
    }
}

private fun formatBytes(bytes: Long): String {
    if (bytes < 1024) return "$bytes B"
    val units = arrayOf("KB", "MB", "GB", "TB")
    var value = bytes.toDouble()
    var unit = -1
    while (value >= 1024 && unit < units.lastIndex) {
        value /= 1024
        unit++
    }
    return "%.1f %s".format(value, units[unit])
}

@Composable
private fun LocalDownloadCard(
    item: LocalDownload,
    onPlay: () -> Unit,
    onDelete: () -> Unit,
) {
    Surface(
        modifier = Modifier.fillMaxWidth().padding(horizontal = 20.dp, vertical = 4.dp),
        color = NightCard.copy(alpha = 0.72f),
        shape = RoundedCornerShape(18.dp),
        border = BorderStroke(1.dp, NightLine.copy(alpha = 0.6f)),
    ) {
        Row(Modifier.padding(12.dp), horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            Box(
                modifier = Modifier
                    .width(54.dp)
                    .aspectRatio(2f / 3f)
                    .clip(RoundedCornerShape(11.dp))
                    .clickable(onClick = onPlay),
            ) {
                CoverImage(
                    url = item.cover,
                    title = item.title,
                    modifier = Modifier.fillMaxWidth().height(81.dp),
                )
                Box(
                    Modifier
                        .align(Alignment.Center)
                        .size(28.dp)
                        .clip(CircleShape)
                        .background(Color.Black.copy(alpha = 0.55f)),
                    contentAlignment = Alignment.Center,
                ) {
                    Icon(
                        Icons.Rounded.PlayArrow,
                        contentDescription = null,
                        tint = Color.White,
                        modifier = Modifier.size(18.dp),
                    )
                }
            }
            Column(Modifier.weight(1f)) {
                Text(
                    item.title,
                    color = Paper,
                    style = MaterialTheme.typography.titleMedium,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                Text(
                    "${item.episode.displayTitle()} · ${Formatter.formatShortFileSize(LocalContext.current, item.size)}",
                    color = Muted,
                    style = MaterialTheme.typography.bodySmall,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                Spacer(Modifier.height(6.dp))
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(7.dp)) {
                    Box(Modifier.size(6.dp).clip(CircleShape).background(Mint))
                    Text("Available offline", color = Mint, style = MaterialTheme.typography.labelSmall)
                }
                Spacer(Modifier.height(8.dp))
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                    Surface(
                        modifier = Modifier
                            .clip(RoundedCornerShape(50))
                            .clickable(onClick = onPlay),
                        color = Ice.copy(alpha = 0.12f),
                        shape = RoundedCornerShape(50),
                    ) {
                        Row(
                            Modifier.padding(horizontal = 11.dp, vertical = 6.dp),
                            verticalAlignment = Alignment.CenterVertically,
                            horizontalArrangement = Arrangement.spacedBy(4.dp),
                        ) {
                            Icon(Icons.Rounded.PlayArrow, contentDescription = null, tint = Ice, modifier = Modifier.size(15.dp))
                            Text("Play", color = Ice, style = MaterialTheme.typography.labelMedium)
                        }
                    }
                    IconButton(onClick = onDelete, modifier = Modifier.size(30.dp)) {
                        Icon(
                            Icons.Rounded.DeleteOutline,
                            contentDescription = "Delete download",
                            tint = Muted,
                            modifier = Modifier.size(19.dp),
                        )
                    }
                }
            }
        }
    }
}