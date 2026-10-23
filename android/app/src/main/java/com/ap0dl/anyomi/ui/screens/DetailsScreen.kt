package com.ap0dl.anyomi.ui.screens

import androidx.compose.animation.animateContentSize
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.ArrowBack
import androidx.compose.material.icons.rounded.Download
import androidx.compose.material.icons.rounded.PlayArrow
import androidx.compose.material.icons.rounded.Refresh
import androidx.compose.material.icons.rounded.SwapVert
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.ap0dl.anyomi.AppViewModel
import com.ap0dl.anyomi.Details
import com.ap0dl.anyomi.data.Anime
import com.ap0dl.anyomi.data.Episode
import com.ap0dl.anyomi.data.Loadable
import com.ap0dl.anyomi.data.SearchHit
import com.ap0dl.anyomi.data.audioLabel
import com.ap0dl.anyomi.data.displayTitle
import com.ap0dl.anyomi.ui.components.CoverImage
import com.ap0dl.anyomi.ui.components.EmptyPanel
import com.ap0dl.anyomi.ui.components.ErrorPanel
import com.ap0dl.anyomi.ui.components.GhostAction
import com.ap0dl.anyomi.ui.components.MetaPill
import com.ap0dl.anyomi.ui.components.PrimaryAction
import com.ap0dl.anyomi.ui.components.SectionHeading
import com.ap0dl.anyomi.ui.components.SkeletonBox
import com.ap0dl.anyomi.ui.components.SourcePill
import com.ap0dl.anyomi.ui.components.metaLine
import com.ap0dl.anyomi.ui.theme.Amber
import com.ap0dl.anyomi.ui.theme.Ice
import com.ap0dl.anyomi.ui.theme.Mint
import com.ap0dl.anyomi.ui.theme.Muted
import com.ap0dl.anyomi.ui.theme.Night
import com.ap0dl.anyomi.ui.theme.NightCard
import com.ap0dl.anyomi.ui.theme.NightLine
import com.ap0dl.anyomi.ui.theme.Paper

@Composable
fun DetailsScreen(
    app: AppViewModel,
    hit: SearchHit,
    onBack: () -> Unit,
    onPlay: (Anime, Episode, List<Episode>) -> Unit,
    onDownload: (Anime, Episode) -> Unit,
) {
    var reload by remember(hit) { mutableIntStateOf(0) }
    val state by produceState<Loadable<Details>>(Loadable.Loading, hit, reload) {
        value = Loadable.Loading
        value = try {
            Loadable.Ready(app.details(hit, force = reload > 0))
        } catch (error: Exception) {
            Loadable.Failed(error.message ?: "Could not open this show")
        }
    }
    val history by app.history.collectAsStateWithLifecycle()

    Box(Modifier.fillMaxSize().background(Night)) {
        when (val current = state) {
            Loadable.Loading, Loadable.Idle -> DetailsSkeleton()
            is Loadable.Failed -> {
                Column(
                    Modifier.fillMaxSize().statusBarsPadding().padding(20.dp),
                    verticalArrangement = Arrangement.Center,
                ) {
                    ErrorPanel(
                        message = current.message,
                        onRetry = { reload++ },
                    )
                    Spacer(Modifier.height(12.dp))
                    GhostAction(text = "Go back", onClick = onBack)
                }
            }
            is Loadable.Ready -> {
                val details = current.value
                DetailsContent(
                    details = details,
                    history = history.firstOrNull {
                        it.anime.url == details.anime.url && it.anime.provider == details.anime.provider
                    },
                    onPlay = onPlay,
                    onDownload = onDownload,
                    onRetryEpisodes = { reload++ },
                )
            }
        }
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .statusBarsPadding()
                .padding(horizontal = 12.dp, vertical = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.SpaceBetween,
        ) {
            RoundIcon(
                icon = Icons.AutoMirrored.Rounded.ArrowBack,
                description = "Back",
                onClick = onBack,
            )
            SourcePill(providerId = hit.provider)
        }
    }
}

@Composable
private fun DetailsContent(
    details: Details,
    history: com.ap0dl.anyomi.data.HistoryItem?,
    onPlay: (Anime, Episode, List<Episode>) -> Unit,
    onDownload: (Anime, Episode) -> Unit,
    onRetryEpisodes: () -> Unit,
) {
    val anime = details.anime
    var expanded by remember(anime.url) { mutableStateOf(false) }
    var newestFirst by remember(anime.url) { mutableStateOf(false) }
    val episodes = remember(details.episodes, newestFirst) {
        if (newestFirst) details.episodes.sortedByDescending { it.number }
        else details.episodes.sortedBy { it.number }
    }
    val resumeEpisode = history?.let { saved ->
        if (saved.completed) {
            details.episodes.firstOrNull { it.number > saved.episode.number }
                ?: details.episodes.firstOrNull { it.number == saved.episode.number }
        } else {
            details.episodes.firstOrNull { it.number == saved.episode.number }
        }
    } ?: details.episodes.firstOrNull()

    LazyColumn(
        modifier = Modifier.fillMaxSize(),
        contentPadding = PaddingValues(bottom = 40.dp),
    ) {
        item {
            Box(
                modifier = Modifier
                    .fillMaxWidth()
                    .height(468.dp),
            ) {
                CoverImage(
                    url = anime.cover,
                    title = anime.title,
                    modifier = Modifier.fillMaxSize(),
                    contentScale = ContentScale.Crop,
                )
                Box(
                    Modifier
                        .fillMaxSize()
                        .background(
                            Brush.verticalGradient(
                                0f to Color(0x33000000),
                                0.48f to Color(0x990B0E14),
                                0.78f to Color(0xF20B0E14),
                                1f to Night,
                            ),
                        ),
                )
                Column(
                    modifier = Modifier
                        .align(Alignment.BottomStart)
                        .fillMaxWidth()
                        .padding(horizontal = 20.dp, vertical = 18.dp),
                ) {
                    Text(
                        anime.title,
                        color = Paper,
                        style = MaterialTheme.typography.headlineLarge,
                        maxLines = 3,
                        overflow = TextOverflow.Ellipsis,
                    )
                    Spacer(Modifier.height(8.dp))
                    Text(
                        metaLine(
                            anime.year?.toString(),
                            anime.format?.replaceFirstChar { it.uppercase() },
                            anime.status?.replaceFirstChar { it.uppercase() },
                            anime.totalEpisodes?.let { "$it episodes" },
                        ),
                        color = Color.White.copy(alpha = 0.72f),
                        style = MaterialTheme.typography.bodyMedium,
                    )
                    if (anime.genres.isNotEmpty()) {
                        Spacer(Modifier.height(10.dp))
                        LazyRow(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                            items(anime.genres.take(6)) { genre ->
                                MetaPill(text = genre)
                            }
                        }
                    }
                }
            }
        }
        item {
            Row(
                modifier = Modifier.fillMaxWidth().padding(horizontal = 20.dp),
                horizontalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                PrimaryAction(
                    text = if (
                        history != null &&
                        !history.completed &&
                        resumeEpisode != null &&
                        resumeEpisode.number == history.episode.number
                    ) {
                        "Resume ${resumeEpisode.label.replace("Episode", "E")}"
                    } else if (resumeEpisode != null) {
                        "Play ${resumeEpisode.label.replace("Episode", "E")}"
                    } else {
                        "No episodes"
                    },
                    modifier = Modifier.weight(1f),
                    enabled = resumeEpisode != null,
                    onClick = {
                        resumeEpisode?.let { onPlay(anime, it, details.episodes) }
                    },
                )
                GhostAction(
                    text = "Latest",
                    modifier = Modifier.width(98.dp),
                    onClick = {
                        details.episodes.maxByOrNull { it.number }?.let { onPlay(anime, it, details.episodes) }
                    },
                )
                Surface(
                    modifier = Modifier
                        .size(48.dp)
                        .clip(RoundedCornerShape(14.dp))
                        .clickable(enabled = resumeEpisode != null) {
                            resumeEpisode?.let { onDownload(anime, it) }
                        },
                    color = Color.White.copy(alpha = 0.06f),
                    shape = RoundedCornerShape(14.dp),
                    border = androidx.compose.foundation.BorderStroke(1.dp, NightLine.copy(alpha = 0.75f)),
                ) {
                    Box(contentAlignment = Alignment.Center) {
                        Icon(
                            Icons.Rounded.Download,
                            contentDescription = "Download episode",
                            tint = if (resumeEpisode != null) Ice else Muted,
                            modifier = Modifier.size(21.dp),
                        )
                    }
                }
            }
            Spacer(Modifier.height(8.dp))
            if (history != null) {
                Text(
                    if (history.completed) {
                        "Completed ${history.episode.label}"
                    } else if (history.durationMs > 0L) {
                        "${history.remainingMinutes} min left · ${history.episode.label}"
                    } else {
                        "Started ${history.episode.label}"
                    },
                    modifier = Modifier.padding(horizontal = 20.dp),
                    color = Mint.copy(alpha = 0.85f),
                    style = MaterialTheme.typography.bodySmall,
                )
            }
            Spacer(Modifier.height(20.dp))
        }
        item {
            Column(Modifier.padding(horizontal = 20.dp)) {
                if (!anime.description.isNullOrBlank()) {
                    Text(
                        "STORY",
                        color = Ice.copy(alpha = 0.7f),
                        style = MaterialTheme.typography.labelMedium,
                        letterSpacing = 1.5.sp,
                    )
                    Spacer(Modifier.height(7.dp))
                    Text(
                        anime.description,
                        color = Paper.copy(alpha = 0.84f),
                        style = MaterialTheme.typography.bodyLarge,
                        maxLines = if (expanded) Int.MAX_VALUE else 5,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.animateContentSize(),
                    )
                    if ((anime.description?.length ?: 0) > 220) {
                        Text(
                            if (expanded) "Show less" else "Read more",
                            modifier = Modifier
                                .padding(top = 5.dp)
                                .clip(RoundedCornerShape(8.dp))
                                .clickable { expanded = !expanded }
                                .padding(vertical = 3.dp),
                            color = Ice,
                            style = MaterialTheme.typography.labelLarge,
                        )
                    }
                    Spacer(Modifier.height(18.dp))
                }
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.SpaceBetween,
                ) {
                    SectionHeading(
                        title = "Episodes",
                        eyebrow = "${details.episodes.size} available · ${details.provider.name}",
                    )
                    Row(
                        modifier = Modifier
                            .clip(RoundedCornerShape(50))
                            .clickable { newestFirst = !newestFirst }
                            .padding(8.dp),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(4.dp),
                    ) {
                        Icon(
                            Icons.Rounded.SwapVert,
                            contentDescription = null,
                            tint = Ice,
                            modifier = Modifier.size(17.dp),
                        )
                        Text(
                            if (newestFirst) "Newest" else "Oldest",
                            color = Ice,
                            style = MaterialTheme.typography.labelMedium,
                        )
                    }
                }
                Spacer(Modifier.height(8.dp))
                if (details.episodeError != null && details.episodes.isEmpty()) {
                    ErrorPanel(
                        message = details.episodeError,
                        onRetry = onRetryEpisodes,
                    )
                } else if (details.episodes.isEmpty()) {
                    EmptyPanel(
                        title = "No episodes listed",
                        message = "This source returned metadata but no playable episodes. Try another source.",
                    )
                }
            }
        }
        items(episodes, key = { "${it.id}:${it.number}" }) { episode ->
            EpisodeRow(
                episode = episode,
                watched = history != null && history.episode.number == episode.number,
                onClick = { onPlay(anime, episode, details.episodes) },
                onDownload = { onDownload(anime, episode) },
            )
        }
        item {
            Spacer(Modifier.height(20.dp))
            Column(Modifier.padding(horizontal = 20.dp)) {
                Surface(
                    color = NightCard.copy(alpha = 0.6f),
                    shape = RoundedCornerShape(16.dp),
                    border = androidx.compose.foundation.BorderStroke(1.dp, NightLine.copy(alpha = 0.65f)),
                ) {
                    Row(
                        modifier = Modifier.padding(15.dp),
                        horizontalArrangement = Arrangement.spacedBy(10.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Box(Modifier.size(7.dp).clip(CircleShape).background(Mint))
                        Text(
                            "Resolved live by the Rust engine · ${details.provider.name}",
                            color = Muted,
                            style = MaterialTheme.typography.bodySmall,
                        )
                    }
                }
            }
        }
    }
}

@Composable
private fun EpisodeRow(
    episode: Episode,
    watched: Boolean,
    onClick: () -> Unit,
    onDownload: () -> Unit,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clickable(onClick = onClick)
            .padding(horizontal = 20.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(13.dp),
    ) {
        Box(
            modifier = Modifier
                .size(43.dp)
                .clip(RoundedCornerShape(13.dp))
                .background(
                    if (watched) Mint.copy(alpha = 0.16f) else Color.White.copy(alpha = 0.055f),
                ),
            contentAlignment = Alignment.Center,
        ) {
            Text(
                episode.number.toInt().toString().padStart(2, '0'),
                color = if (watched) Mint else Paper,
                style = MaterialTheme.typography.titleMedium,
                fontWeight = FontWeight.Black,
            )
        }
        Column(Modifier.weight(1f)) {
            Text(
                episode.displayTitle(),
                color = Paper,
                style = MaterialTheme.typography.titleMedium,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Spacer(Modifier.height(3.dp))
            Row(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
                Text(
                    episode.audio.audioLabel(),
                    color = Muted,
                    style = MaterialTheme.typography.labelSmall,
                )
                if (episode.filler) Text("FILLER", color = Amber, style = MaterialTheme.typography.labelSmall)
                if (episode.recap) Text("RECAP", color = Amber, style = MaterialTheme.typography.labelSmall)
                if (watched) Text("WATCHED", color = Mint, style = MaterialTheme.typography.labelSmall)
                episode.aired?.take(10)?.let {
                    Text(it, color = Muted.copy(alpha = 0.62f), style = MaterialTheme.typography.labelSmall)
                }
            }
        }
        Box(
            modifier = Modifier
                .size(34.dp)
                .clip(CircleShape)
                .background(Ice.copy(alpha = 0.12f)),
            contentAlignment = Alignment.Center,
        ) {
            Icon(
                Icons.Rounded.PlayArrow,
                contentDescription = "Play ${episode.label}",
                tint = Ice,
                modifier = Modifier.size(20.dp),
            )
        }
        IconButton(onClick = onDownload, modifier = Modifier.size(34.dp)) {
            Icon(
                Icons.Rounded.Download,
                contentDescription = "Download ${episode.label}",
                tint = Muted,
                modifier = Modifier.size(19.dp),
            )
        }
    }
}

@Composable
private fun RoundIcon(
    icon: androidx.compose.ui.graphics.vector.ImageVector,
    description: String,
    onClick: () -> Unit,
) {
    Surface(
        color = Color.Black.copy(alpha = 0.42f),
        shape = CircleShape,
        modifier = Modifier.size(40.dp),
    ) {
        IconButton(onClick = onClick) {
            Icon(icon, contentDescription = description, tint = Color.White)
        }
    }
}

@Composable
private fun DetailsSkeleton() {
    Column(Modifier.fillMaxSize()) {
        SkeletonBox(modifier = Modifier.fillMaxWidth().aspectRatio(0.84f), corner = 0)
        Column(Modifier.padding(20.dp)) {
            SkeletonBox(modifier = Modifier.fillMaxWidth(0.72f).height(28.dp), corner = 7)
            Spacer(Modifier.height(10.dp))
            SkeletonBox(modifier = Modifier.fillMaxWidth(0.45f).height(13.dp), corner = 5)
            Spacer(Modifier.height(20.dp))
            SkeletonBox(modifier = Modifier.fillMaxWidth().height(48.dp), corner = 14)
            Spacer(Modifier.height(22.dp))
            repeat(5) {
                SkeletonBox(
                    modifier = Modifier.fillMaxWidth().height(58.dp).padding(vertical = 4.dp),
                    corner = 14,
                )
            }
        }
    }
}