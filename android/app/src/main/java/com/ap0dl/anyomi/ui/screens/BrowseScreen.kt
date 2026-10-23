package com.ap0dl.anyomi.ui.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items as lazyItems
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.GridItemSpan
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items as gridItems
import androidx.compose.foundation.lazy.grid.itemsIndexed
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.AutoAwesome
import androidx.compose.material.icons.rounded.Search
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.ap0dl.anyomi.AppViewModel
import com.ap0dl.anyomi.data.Loadable
import com.ap0dl.anyomi.data.ProviderInfo
import com.ap0dl.anyomi.data.SearchHit
import com.ap0dl.anyomi.ui.components.EmptyPanel
import com.ap0dl.anyomi.ui.components.ErrorPanel
import com.ap0dl.anyomi.ui.components.PosterCard
import com.ap0dl.anyomi.ui.components.SearchInput
import com.ap0dl.anyomi.ui.components.SectionHeading
import com.ap0dl.anyomi.ui.components.SkeletonPoster
import com.ap0dl.anyomi.ui.components.SourceCard
import com.ap0dl.anyomi.ui.components.providerColor
import com.ap0dl.anyomi.ui.components.providerLabel
import com.ap0dl.anyomi.ui.theme.Ice
import com.ap0dl.anyomi.ui.theme.Mint
import com.ap0dl.anyomi.ui.theme.Muted
import com.ap0dl.anyomi.ui.theme.Night
import com.ap0dl.anyomi.ui.theme.NightCard
import com.ap0dl.anyomi.ui.theme.NightLine
import com.ap0dl.anyomi.ui.theme.Paper

private val suggestions = listOf("Frieren", "Cyberpunk", "One Piece", "Jujutsu Kaisen", "Solo Leveling")

@Composable
fun BrowseScreen(
    app: AppViewModel,
    navProvider: String?,
    onOpen: (SearchHit) -> Unit,
    vm: BrowseViewModel = viewModel(),
) {
    val providers by app.providers.collectAsStateWithLifecycle()
    val query by vm.query.collectAsStateWithLifecycle()
    val selectedProvider by vm.provider.collectAsStateWithLifecycle()
    val results by vm.results.collectAsStateWithLifecycle()
    val keyboard = LocalSoftwareKeyboardController.current
    val providerValues = (providers as? Loadable.Ready)?.value.orEmpty()

    LaunchedEffect(navProvider) {
        vm.applyNavProvider(navProvider)
    }

    Column(
        modifier = Modifier.fillMaxSize().background(Night).statusBarsPadding(),
    ) {
        Row(
            modifier = Modifier.fillMaxWidth().padding(horizontal = 20.dp, vertical = 16.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.SpaceBetween,
        ) {
            Column {
                Text(
                    "SEARCH",
                    color = Mint,
                    style = MaterialTheme.typography.labelMedium,
                    letterSpacing = 2.sp,
                )
                Text(
                    "Search",
                    color = Paper,
                    style = MaterialTheme.typography.headlineLarge,
                )
            }
            Surface(
                color = Ice.copy(alpha = 0.1f),
                shape = RoundedCornerShape(50),
            ) {
                Row(
                    modifier = Modifier.padding(horizontal = 10.dp, vertical = 6.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(5.dp),
                ) {
                    Icon(
                        Icons.Rounded.AutoAwesome,
                        contentDescription = null,
                        tint = Ice,
                        modifier = Modifier.size(14.dp),
                    )
                    Text(
                        "${providerValues.size.coerceAtLeast(0)} SOURCES",
                        color = Ice,
                        style = MaterialTheme.typography.labelSmall,
                    )
                }
            }
        }
        SearchInput(
            value = query,
            onValueChange = vm::onQueryChange,
            onSearch = {
                keyboard?.hide()
                vm.submitQuery()
            },
            modifier = Modifier.padding(horizontal = 20.dp),
            placeholder = if (selectedProvider == null) "Search every source…" else "Search ${providerLabel(selectedProvider!!)}…",
        )
        Spacer(Modifier.height(12.dp))
        LazyRow(
            contentPadding = PaddingValues(horizontal = 20.dp),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            item {
                ProviderChip(
                    label = "All sources",
                    color = Ice,
                    selected = selectedProvider == null,
                    onClick = {
                        vm.selectProvider(null)
                        keyboard?.hide()
                    },
                )
            }
            lazyItems(providerValues, key = { it.id }) { provider ->
                ProviderChip(
                    label = providerLabel(provider.id, provider.name),
                    color = providerColor(provider.id),
                    selected = selectedProvider == provider.id,
                    onClick = {
                        vm.selectProvider(provider.id)
                        keyboard?.hide()
                    },
                )
            }
        }
        Spacer(Modifier.height(14.dp))

        Box(Modifier.fillMaxSize()) {
            when (val state = results) {
                Loadable.Idle -> {
                    if (query.isBlank()) {
                        DiscoveryGrid(
                            providers = providerValues,
                            onSource = { vm.selectProvider(it.id) },
                            onSuggestion = {
                                keyboard?.hide()
                                vm.suggest(it)
                            },
                        )
                    } else {
                        EmptyPanel(
                            title = "Keep typing",
                            message = "Enter at least two characters to search every live source.",
                            modifier = Modifier.padding(horizontal = 20.dp),
                        )
                    }
                }
                Loadable.Loading -> LoadingGrid()
                is Loadable.Failed -> Column(Modifier.padding(horizontal = 20.dp)) {
                    ErrorPanel(message = state.message, onRetry = vm::retry)
                }
                is Loadable.Ready -> {
                    if (state.value.isEmpty()) {
                        EmptyPanel(
                            title = "Nothing found",
                            message = "Try fewer words, a romanized title, or a different source.",
                            modifier = Modifier.padding(horizontal = 20.dp),
                        )
                    } else {
                        ResultsGrid(
                            hits = state.value,
                            query = query,
                            provider = selectedProvider,
                            onOpen = onOpen,
                        )
                    }
                }
            }
        }
    }
}

@Composable
private fun ProviderChip(
    label: String,
    color: Color,
    selected: Boolean,
    onClick: () -> Unit,
) {
    Surface(
        modifier = Modifier.clip(RoundedCornerShape(50)).clickable(onClick = onClick),
        color = if (selected) color.copy(alpha = 0.18f) else NightCard.copy(alpha = 0.5f),
        shape = RoundedCornerShape(50),
        border = androidx.compose.foundation.BorderStroke(
            1.dp,
            if (selected) color.copy(alpha = 0.55f) else NightLine.copy(alpha = 0.65f),
        ),
    ) {
        Row(
            modifier = Modifier.padding(horizontal = 13.dp, vertical = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(7.dp),
        ) {
            Box(
                Modifier.size(6.dp).clip(CircleShape)
                    .background(if (selected) color else color.copy(alpha = 0.65f)),
            )
            Text(
                label,
                color = if (selected) color else Muted,
                style = MaterialTheme.typography.labelLarge,
            )
        }
    }
}

@Composable
private fun DiscoveryGrid(
    providers: List<ProviderInfo>,
    onSource: (ProviderInfo) -> Unit,
    onSuggestion: (String) -> Unit,
) {
    LazyVerticalGrid(
        columns = GridCells.Fixed(2),
        modifier = Modifier.fillMaxSize(),
        contentPadding = PaddingValues(start = 20.dp, end = 20.dp, bottom = 28.dp),
        horizontalArrangement = Arrangement.spacedBy(10.dp),
        verticalArrangement = Arrangement.spacedBy(2.dp),
    ) {
        item(span = { GridItemSpan(maxLineSpan) }) {
            Surface(
                modifier = Modifier.fillMaxWidth(),
                shape = RoundedCornerShape(24.dp),
                color = Color.Transparent,
            ) {
                Column(
                    modifier = Modifier
                        .background(
                            Brush.linearGradient(
                                listOf(Color(0xFF1C2C4D), Color(0xFF141A27)),
                            ),
                        )
                        .padding(20.dp),
                ) {
                    Icon(
                        Icons.Rounded.Search,
                        contentDescription = null,
                        tint = Mint,
                        modifier = Modifier.size(25.dp),
                    )
                    Spacer(Modifier.height(14.dp))
                    Text(
                        "Search every source.",
                        color = Paper,
                        style = MaterialTheme.typography.headlineMedium,
                    )
                    Spacer(Modifier.height(7.dp))
                    Text(
                        "Find a title, then pick the source you want to watch.",
                        color = Muted,
                        style = MaterialTheme.typography.bodyMedium,
                    )
                    Spacer(Modifier.height(15.dp))
                    Text(
                        "POPULAR SEARCHES",
                        color = Ice.copy(alpha = 0.75f),
                        style = MaterialTheme.typography.labelSmall,
                        letterSpacing = 1.2.sp,
                    )
                    Spacer(Modifier.height(8.dp))
                    LazyRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        lazyItems(suggestions) { suggestion ->
                            Surface(
                                modifier = Modifier.clip(RoundedCornerShape(50)).clickable {
                                    onSuggestion(suggestion)
                                },
                                color = Color.White.copy(alpha = 0.07f),
                                shape = RoundedCornerShape(50),
                            ) {
                                Text(
                                    suggestion,
                                    modifier = Modifier.padding(horizontal = 11.dp, vertical = 7.dp),
                                    color = Paper.copy(alpha = 0.9f),
                                    style = MaterialTheme.typography.labelMedium,
                                )
                            }
                        }
                    }
                }
            }
        }
        item(span = { GridItemSpan(maxLineSpan) }) {
            SectionHeading(
                title = "Live sources",
                eyebrow = "Tap to browse latest",
                modifier = Modifier.padding(top = 20.dp, bottom = 8.dp),
            )
        }
        if (providers.isEmpty()) {
            items(4) {
                com.ap0dl.anyomi.ui.components.SkeletonBox(
                    modifier = Modifier.fillMaxWidth().height(150.dp),
                    corner = 20,
                )
            }
        } else {
            gridItems(providers, key = { it.id }) { provider ->
                SourceCard(
                    provider = provider,
                    onClick = { onSource(provider) },
                    modifier = Modifier.padding(vertical = 4.dp),
                )
            }
        }
    }
}

@Composable
private fun LoadingGrid() {
    LazyVerticalGrid(
        columns = GridCells.Adaptive(132.dp),
        modifier = Modifier.fillMaxSize(),
        contentPadding = PaddingValues(start = 20.dp, end = 20.dp, bottom = 28.dp),
        horizontalArrangement = Arrangement.spacedBy(12.dp),
        verticalArrangement = Arrangement.spacedBy(18.dp),
    ) {
        items(9) {
            SkeletonPoster()
        }
    }
}

@Composable
private fun ResultsGrid(
    hits: List<SearchHit>,
    query: String,
    provider: String?,
    onOpen: (SearchHit) -> Unit,
) {
    LazyVerticalGrid(
        columns = GridCells.Adaptive(132.dp),
        modifier = Modifier.fillMaxSize(),
        contentPadding = PaddingValues(start = 20.dp, end = 20.dp, bottom = 28.dp),
        horizontalArrangement = Arrangement.spacedBy(12.dp),
        verticalArrangement = Arrangement.spacedBy(18.dp),
    ) {
        item(span = { GridItemSpan(maxLineSpan) }) {
            SectionHeading(
                title = if (query.isBlank() && provider != null) {
                    "New on ${providerLabel(provider)}"
                } else {
                    "Results for \"$query\""
                },
                eyebrow = "${hits.size} matches",
                modifier = Modifier.padding(bottom = 2.dp),
            )
        }
        itemsIndexed(hits, key = { index, hit -> "${hit.provider}:${hit.url}:$index" }) { _, hit ->
            PosterCard(hit = hit, onClick = { onOpen(hit) })
        }
    }
}