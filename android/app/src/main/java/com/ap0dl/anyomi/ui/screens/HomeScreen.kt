package com.ap0dl.anyomi.ui.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
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
import androidx.compose.material.icons.rounded.ArrowForward
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.ap0dl.anyomi.AppViewModel
import com.ap0dl.anyomi.BuildConfig
import com.ap0dl.anyomi.data.HistoryItem
import com.ap0dl.anyomi.data.Loadable
import com.ap0dl.anyomi.data.SearchHit
import com.ap0dl.anyomi.ui.components.ErrorPanel
import com.ap0dl.anyomi.ui.components.HeroCard
import com.ap0dl.anyomi.ui.components.HistoryCard
import com.ap0dl.anyomi.ui.components.LogoMark
import com.ap0dl.anyomi.ui.components.PosterCard
import com.ap0dl.anyomi.ui.components.ReadOnlySearchBar
import com.ap0dl.anyomi.ui.components.SectionHeading
import com.ap0dl.anyomi.ui.components.SkeletonPoster
import com.ap0dl.anyomi.ui.components.SourceCard
import com.ap0dl.anyomi.ui.components.providerLabel
import com.ap0dl.anyomi.ui.theme.Ice
import com.ap0dl.anyomi.ui.theme.Mint
import com.ap0dl.anyomi.ui.theme.Muted
import com.ap0dl.anyomi.ui.theme.Night
import com.ap0dl.anyomi.ui.theme.Paper

@Composable
fun HomeScreen(
    app: AppViewModel,
    onOpen: (SearchHit) -> Unit,
    onBrowse: () -> Unit,
    onBrowseSource: (String) -> Unit,
    onResume: (HistoryItem) -> Unit,
) {
    val latest by app.latest.collectAsStateWithLifecycle()
    val providers by app.providers.collectAsStateWithLifecycle()
    val history by app.history.collectAsStateWithLifecycle()
    val providerValues = (providers as? Loadable.Ready)?.value.orEmpty()
    val inProgress = history.count { it.progress < 0.95f }

    LazyColumn(
        modifier = Modifier.fillMaxWidth().background(Night).statusBarsPadding(),
        contentPadding = PaddingValues(bottom = 28.dp),
    ) {
        item {
            Header(
                sourceCount = providerValues.size,
                onBrowse = onBrowse,
            )
        }
        item {
            Column(Modifier.padding(horizontal = 20.dp)) {
                Spacer(Modifier.height(14.dp))
                ReadOnlySearchBar(
                    hint = "Search anime",
                    onClick = onBrowse,
                )
            }
        }
        item {
            Spacer(Modifier.height(22.dp))
            when (val state = latest) {
                Loadable.Loading, Loadable.Idle -> HeroSkeleton()
                is Loadable.Failed -> {
                    Column(Modifier.padding(horizontal = 20.dp)) {
                        ErrorPanel(message = state.message, onRetry = { app.refreshLatest() })
                    }
                }
                is Loadable.Ready -> {
                    state.value.firstOrNull()?.let { hero ->
                        HeroCard(
                            hit = hero,
                            onOpen = { onOpen(hero) },
                            modifier = Modifier.padding(horizontal = 20.dp),
                        )
                    }
                }
            }
        }
        if (history.isNotEmpty()) {
            item {
                Spacer(Modifier.height(30.dp))
                SectionHeading(
                    title = "Continue watching",
                    eyebrow = if (inProgress > 0) "$inProgress in progress" else "Recently watched",
                    modifier = Modifier.padding(horizontal = 20.dp),
                )
                Spacer(Modifier.height(14.dp))
                LazyRow(
                    contentPadding = PaddingValues(horizontal = 20.dp),
                    horizontalArrangement = Arrangement.spacedBy(12.dp),
                ) {
                    items(
                        history.take(10),
                        key = { "${it.anime.provider}:${it.anime.url}:${it.episode.number}" },
                    ) { item ->
                        HistoryCard(item = item, onPlay = { onResume(item) })
                    }
                }
            }
        }
        val ready = latest as? Loadable.Ready
        if (ready != null && ready.value.size > 1) {
            item {
                Spacer(Modifier.height(30.dp))
                SectionHeading(
                    title = "Fresh episodes",
                    eyebrow = "New on ${providerLabel(ready.value.first().provider)}",
                    action = "Browse",
                    onAction = onBrowse,
                    modifier = Modifier.padding(horizontal = 20.dp),
                )
                Spacer(Modifier.height(14.dp))
                LazyRow(
                    contentPadding = PaddingValues(horizontal = 20.dp),
                    horizontalArrangement = Arrangement.spacedBy(12.dp),
                ) {
                    items(ready.value.drop(1), key = { "${it.provider}:${it.url}" }) { hit ->
                        PosterCard(
                            hit = hit,
                            onClick = { onOpen(hit) },
                            modifier = Modifier.width(146.dp),
                        )
                    }
                }
            }
        }
        item {
            Spacer(Modifier.height(30.dp))
            SectionHeading(
                title = "Where to watch",
                eyebrow = if (providerValues.isEmpty()) "Loading sources" else "${providerValues.size} sources",
                action = "All sources",
                onAction = onBrowse,
                modifier = Modifier.padding(horizontal = 20.dp),
            )
            Spacer(Modifier.height(12.dp))
            Column(Modifier.padding(horizontal = 20.dp)) {
                providerValues.take(6).chunked(2).forEach { row ->
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.spacedBy(10.dp),
                    ) {
                        row.forEach { provider ->
                            SourceCard(
                                provider = provider,
                                onClick = { onBrowseSource(provider.id) },
                                modifier = Modifier.weight(1f).padding(vertical = 5.dp),
                            )
                        }
                        if (row.size == 1) Spacer(Modifier.weight(1f))
                    }
                }
                if (providerValues.isEmpty()) {
                    Text(
                        "Sources are loading…",
                        color = Muted,
                        style = MaterialTheme.typography.bodyMedium,
                    )
                }
            }
        }
        item {
            Spacer(Modifier.height(32.dp))
            Row(
                modifier = Modifier.fillMaxWidth().padding(horizontal = 20.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.SpaceBetween,
            ) {
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(9.dp),
                ) {
                    LogoMark(size = 22)
                    Column {
                        Text(
                            "AP0DL",
                            color = Paper.copy(alpha = 0.78f),
                            style = MaterialTheme.typography.labelMedium,
                            letterSpacing = 2.4.sp,
                            fontWeight = FontWeight.Bold,
                        )
                        Text(
                            "MADE WITH A RUST HEART",
                            color = Muted.copy(alpha = 0.58f),
                            style = MaterialTheme.typography.labelSmall,
                            letterSpacing = 0.7.sp,
                        )
                    }
                }
                Text(
                    "v${BuildConfig.VERSION_NAME}",
                    color = Muted.copy(alpha = 0.52f),
                    style = MaterialTheme.typography.labelSmall,
                )
            }
        }
    }
}

@Composable
private fun Header(sourceCount: Int, onBrowse: () -> Unit) {
    Row(
        modifier = Modifier.fillMaxWidth().padding(horizontal = 20.dp).padding(top = 16.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.SpaceBetween,
    ) {
        Row(
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            LogoMark(size = 38)
            Column {
                Text(
                    "AP0DL",
                    color = Paper,
                    style = MaterialTheme.typography.headlineSmall,
                    letterSpacing = 1.4.sp,
                    fontWeight = FontWeight.Black,
                )
                Text(
                    "v${BuildConfig.VERSION_NAME}",
                    color = Muted,
                    style = MaterialTheme.typography.labelSmall,
                    letterSpacing = 1.2.sp,
                )
            }
        }
        Row(
            modifier = Modifier
                .clip(CircleShape)
                .background(Mint.copy(alpha = 0.08f))
                .padding(horizontal = 10.dp, vertical = 6.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            Box(Modifier.size(6.dp).clip(CircleShape).background(Mint))
            Text(
                if (sourceCount > 0) "$sourceCount SOURCES" else "SOURCES",
                color = Mint,
                style = MaterialTheme.typography.labelSmall,
                letterSpacing = 0.8.sp,
            )
        }
    }
}

@Composable
private fun HeroSkeleton() {
    Column(Modifier.padding(horizontal = 20.dp)) {
        SkeletonPoster(modifier = Modifier.fillMaxWidth().height(410.dp))
        Spacer(Modifier.height(10.dp))
    }
}