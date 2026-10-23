package com.ap0dl.anyomi.ui.components

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.ArrowForward
import androidx.compose.material.icons.rounded.Check
import androidx.compose.material.icons.rounded.PlayArrow
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import coil.compose.AsyncImage
import coil.request.ImageRequest
import com.ap0dl.anyomi.data.HistoryItem
import com.ap0dl.anyomi.data.ProviderInfo
import com.ap0dl.anyomi.data.SearchHit
import com.ap0dl.anyomi.data.audioLabel
import com.ap0dl.anyomi.ui.theme.HeroGradient
import com.ap0dl.anyomi.ui.theme.Ice
import com.ap0dl.anyomi.ui.theme.Mint
import com.ap0dl.anyomi.ui.theme.Muted
import com.ap0dl.anyomi.ui.theme.Night
import com.ap0dl.anyomi.ui.theme.NightCard
import com.ap0dl.anyomi.ui.theme.NightLine
import com.ap0dl.anyomi.ui.theme.Paper

@Composable
fun CoverImage(
    url: String?,
    title: String,
    modifier: Modifier = Modifier,
    contentScale: ContentScale = ContentScale.Crop,
) {
    Box(
        modifier = modifier.background(
            Brush.linearGradient(
                listOf(Color(0xFF26324B), Color(0xFF141A28), Color(0xFF0D111A)),
            ),
        ),
    ) {
        Text(
            text = title.trim().firstOrNull()?.uppercase() ?: "?",
            modifier = Modifier.align(Alignment.Center),
            color = Color.White.copy(alpha = 0.18f),
            style = MaterialTheme.typography.displayLarge,
            fontWeight = FontWeight.Black,
        )
        if (!url.isNullOrBlank()) {
            AsyncImage(
                model = ImageRequest.Builder(LocalContext.current)
                    .data(url)
                    .crossfade(260)
                    .build(),
                contentDescription = title,
                contentScale = contentScale,
                modifier = Modifier.matchParentSize(),
            )
        }
    }
}

@Composable
fun PosterCard(
    hit: SearchHit,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    progress: Float? = null,
    showProvider: Boolean = true,
    subtitleOverride: String? = null,
) {
    Column(
        // Only the artwork is clipped. Clipping the whole column rounded off
        // the first character of metadata such as "2026 · TV".
        modifier = modifier.clickable(onClick = onClick),
    ) {
        Box(
            modifier = Modifier
                .fillMaxWidth()
                .aspectRatio(2f / 3f)
                .clip(RoundedCornerShape(18.dp)),
        ) {
            CoverImage(url = hit.cover, title = hit.title, modifier = Modifier.fillMaxSize())
            Box(
                Modifier
                    .matchParentSize()
                    .background(
                        Brush.verticalGradient(
                            0.55f to Color.Transparent,
                            1f to Color(0xB30B0E14),
                        ),
                    ),
            )
            if (showProvider) {
                SourcePill(
                    providerId = hit.provider,
                    modifier = Modifier.align(Alignment.TopStart).padding(8.dp),
                    compact = true,
                )
            }
            val meta = subtitleOverride ?: metaLine(
                hit.year?.toString(),
                hit.format?.replaceFirstChar { it.uppercase() },
                hit.episodes?.let { "$it EP" },
            )
            if (meta.isNotBlank()) {
                Text(
                    text = meta,
                    modifier = Modifier
                        .align(Alignment.BottomStart)
                        .padding(horizontal = 10.dp, vertical = 8.dp),
                    color = Color.White.copy(alpha = 0.9f),
                    style = MaterialTheme.typography.labelSmall,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
            if (progress != null && progress > 0f) {
                LinearProgressIndicator(
                    progress = { progress },
                    modifier = Modifier
                        .align(Alignment.BottomCenter)
                        .fillMaxWidth()
                        .height(3.dp),
                    color = Mint,
                    trackColor = Color.White.copy(alpha = 0.18f),
                    drawStopIndicator = {},
                )
            }
        }
        Spacer(Modifier.height(9.dp))
        Text(
            text = hit.title,
            color = Paper,
            style = MaterialTheme.typography.titleMedium,
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
        )
        Spacer(Modifier.height(3.dp))
        Text(
            text = metaLine(
                hit.year?.toString(),
                hit.format?.replaceFirstChar { it.uppercase() },
            ).ifBlank { providerLabel(hit.provider) },
            color = Muted,
            style = MaterialTheme.typography.bodySmall,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

@Composable
fun HeroCard(
    hit: SearchHit,
    onOpen: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Box(
        modifier = modifier
            .fillMaxWidth()
            .height(410.dp)
            .clip(RoundedCornerShape(26.dp))
            .clickable(onClick = onOpen),
    ) {
        CoverImage(
            url = hit.cover,
            title = hit.title,
            modifier = Modifier.fillMaxSize(),
            contentScale = ContentScale.Crop,
        )
        Box(
            Modifier
                .fillMaxSize()
                .background(
                    Brush.verticalGradient(
                        0f to Color(0x22000000),
                        0.42f to Color(0x440B0E14),
                        0.75f to Color(0xE60B0E14),
                        1f to Night,
                    ),
                ),
        )
        SourcePill(
            providerId = hit.provider,
            modifier = Modifier.align(Alignment.TopStart).padding(16.dp),
        )
        Column(
            modifier = Modifier
                .align(Alignment.BottomStart)
                .fillMaxWidth()
                .padding(18.dp),
        ) {
            Text(
                "FEATURED",
                color = Mint,
                style = MaterialTheme.typography.labelMedium,
                letterSpacing = 2.sp,
            )
            Spacer(Modifier.height(6.dp))
            Text(
                hit.title,
                color = Paper,
                style = MaterialTheme.typography.headlineLarge,
                maxLines = 3,
                overflow = TextOverflow.Ellipsis,
            )
            Spacer(Modifier.height(8.dp))
            Text(
                metaLine(
                    hit.year?.toString(),
                    hit.format?.replaceFirstChar { it.uppercase() },
                    hit.episodes?.let { "$it episodes" },
                ),
                color = Color.White.copy(alpha = 0.74f),
                style = MaterialTheme.typography.bodyMedium,
            )
            Spacer(Modifier.height(14.dp))
            Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                PrimaryAction(text = "View details", onClick = onOpen)
            }
        }
    }
}

@Composable
fun HistoryCard(
    item: HistoryItem,
    onPlay: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val hit = SearchHit(
        id = item.anime.id,
        title = item.anime.title,
        url = item.anime.url,
        provider = item.anime.provider,
        providerName = item.anime.provider,
        cover = item.anime.cover,
        year = item.anime.year,
        format = item.anime.format,
        episodes = item.anime.totalEpisodes,
    )
    Column(
        // Keep the text outside the rounded clip for the same reason as
        // PosterCard: years and episode labels must not lose their first digit.
        modifier = modifier
            .width(160.dp)
            .clickable(onClick = onPlay),
    ) {
        Box(
            Modifier
                .fillMaxWidth()
                .aspectRatio(2f / 3f)
                .clip(RoundedCornerShape(18.dp)),
        ) {
            CoverImage(url = item.anime.cover, title = item.anime.title, modifier = Modifier.fillMaxSize())
            Box(
                Modifier
                    .matchParentSize()
                    .background(
                        Brush.verticalGradient(
                            0.55f to Color.Transparent,
                            1f to Color(0xCC0B0E14),
                        ),
                    ),
            )
            Box(
                Modifier
                    .align(Alignment.Center)
                    .size(46.dp)
                    .clip(CircleShape)
                    .background(Color.Black.copy(alpha = 0.5f)),
                contentAlignment = Alignment.Center,
            ) {
                Icon(
                    Icons.Rounded.PlayArrow,
                    contentDescription = null,
                    tint = Color.White,
                    modifier = Modifier.size(28.dp),
                )
            }
            Text(
                item.episode.label,
                modifier = Modifier
                    .align(Alignment.BottomStart)
                    .padding(10.dp),
                color = Color.White.copy(alpha = 0.92f),
                style = MaterialTheme.typography.labelMedium,
            )
            Box(
                modifier = Modifier
                    .align(Alignment.BottomEnd)
                    .padding(9.dp)
                    .size(46.dp),
                contentAlignment = Alignment.Center,
            ) {
                CircularProgressIndicator(
                    progress = { item.progress },
                    modifier = Modifier.fillMaxSize(),
                    color = if (item.completed) Mint else Ice,
                    trackColor = Color.White.copy(alpha = 0.18f),
                    strokeWidth = 3.dp,
                )
                if (item.completed) {
                    Icon(
                        Icons.Rounded.Check,
                        contentDescription = "Completed",
                        tint = Mint,
                        modifier = Modifier.size(19.dp),
                    )
                } else {
                    Text(
                        if (item.durationMs > 0L) "${item.remainingMinutes}m" else "NEW",
                        color = Color.White,
                        style = MaterialTheme.typography.labelSmall,
                        fontWeight = FontWeight.Bold,
                    )
                }
            }
        }
        Spacer(Modifier.height(8.dp))
        Text(
            item.anime.title,
            color = Paper,
            style = MaterialTheme.typography.titleMedium,
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
        )
        Row(
            modifier = Modifier.fillMaxWidth(),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.SpaceBetween,
        ) {
            Text(
                item.episode.audio.audioLabel(),
                color = Muted,
                style = MaterialTheme.typography.bodySmall,
            )
            Text(
                when {
                    item.completed -> "Completed"
                    item.durationMs > 0L -> "${item.remainingMinutes} min left"
                    else -> "Just started"
                },
                color = if (item.completed) Mint else Ice.copy(alpha = 0.86f),
                style = MaterialTheme.typography.labelSmall,
                fontWeight = FontWeight.SemiBold,
            )
        }
        Spacer(Modifier.height(2.dp))
        Text(
            providerLabel(item.anime.provider),
            color = providerColor(item.anime.provider),
            style = MaterialTheme.typography.labelSmall,
        )
    }
}

/**
 * Short, user-facing source blurbs. Rust keeps the detailed adapter notes for
 * the CLI; the app should describe what a viewer gets, not how it is scraped.
 */
private fun providerSummary(provider: ProviderInfo): String = when (provider.id.lowercase()) {
    "reanime" -> "Sub and Dub streams with a large catalog."
    "animegg" -> "Direct MP4 playback with multiple qualities."
    "anidb" -> "HLS and 720p MP4 with complete episode lists."
    "kisskh" -> "HLS streams with per-episode keys."
    "animeparadise" -> "Opaque-token HLS with 1080p variants."
    "animetoki" -> "MKV playback · experimental while quotas are fixed."
    else -> provider.notes.ifBlank {
        provider.baseUrl.substringAfter("://").substringBefore("/")
    }
}

@Composable
fun SourceCard(
    provider: ProviderInfo,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val accent = providerColor(provider.id)
    Surface(
        modifier = modifier.clip(RoundedCornerShape(20.dp)).clickable(onClick = onClick),
        color = NightCard.copy(alpha = 0.82f),
        shape = RoundedCornerShape(20.dp),
        border = BorderStroke(1.dp, NightLine.copy(alpha = 0.75f)),
    ) {
        Column(Modifier.padding(16.dp)) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Box(
                    Modifier.size(10.dp).clip(CircleShape).background(accent),
                )
                Icon(
                    Icons.Rounded.ArrowForward,
                    contentDescription = null,
                    tint = Muted,
                    modifier = Modifier.size(17.dp),
                )
            }
            Spacer(Modifier.height(14.dp))
            Text(
                providerLabel(provider.id, provider.name),
                color = Paper,
                style = MaterialTheme.typography.titleLarge,
            )
            Spacer(Modifier.height(4.dp))
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(5.dp)) {
                Box(Modifier.size(5.dp).clip(CircleShape).background(statusColor(provider.status)))
                Text(
                    provider.status.replaceFirstChar { it.uppercase() },
                    color = Muted,
                    style = MaterialTheme.typography.labelSmall,
                )
            }
            Spacer(Modifier.height(10.dp))
            Text(
                providerSummary(provider),
                color = Muted.copy(alpha = 0.82f),
                style = MaterialTheme.typography.bodySmall,
                maxLines = 3,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }
}

@Composable
fun ProviderRow(provider: ProviderInfo, onClick: (() -> Unit)? = null) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .then(if (onClick != null) Modifier.clickable(onClick = onClick) else Modifier)
            .padding(vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Box(
            Modifier
                .size(36.dp)
                .clip(RoundedCornerShape(12.dp))
                .background(providerColor(provider.id).copy(alpha = 0.16f)),
            contentAlignment = Alignment.Center,
        ) {
            Text(
                providerLabel(provider.id, provider.name).take(1),
                color = providerColor(provider.id),
                style = MaterialTheme.typography.titleMedium,
                fontWeight = FontWeight.Black,
            )
        }
        Column(Modifier.weight(1f)) {
            Text(providerLabel(provider.id, provider.name), color = Paper, style = MaterialTheme.typography.titleMedium)
            Text(
                provider.baseUrl.substringAfter("://").substringBefore("/"),
                color = Muted,
                style = MaterialTheme.typography.bodySmall,
            )
        }
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(5.dp)) {
            Box(Modifier.size(6.dp).clip(CircleShape).background(statusColor(provider.status)))
            Text(provider.status, color = Muted, style = MaterialTheme.typography.labelSmall)
        }
    }
}