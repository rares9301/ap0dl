package com.ap0dl.anyomi.ui.screens

import android.widget.Toast
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
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
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.ArrowDownward
import androidx.compose.material.icons.rounded.ArrowUpward
import androidx.compose.material.icons.rounded.Refresh
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.ap0dl.anyomi.AppViewModel
import com.ap0dl.anyomi.data.Loadable
import com.ap0dl.anyomi.data.SearchHit
import com.ap0dl.anyomi.data.TrendingChart
import com.ap0dl.anyomi.data.TrendingEntry
import com.ap0dl.anyomi.ui.components.CoverImage
import com.ap0dl.anyomi.ui.components.ErrorPanel
import com.ap0dl.anyomi.ui.components.SkeletonBox
import com.ap0dl.anyomi.ui.theme.Amber
import com.ap0dl.anyomi.ui.theme.Ice
import com.ap0dl.anyomi.ui.theme.Mint
import com.ap0dl.anyomi.ui.theme.Muted
import com.ap0dl.anyomi.ui.theme.Night
import com.ap0dl.anyomi.ui.theme.NightCard
import com.ap0dl.anyomi.ui.theme.NightLine
import com.ap0dl.anyomi.ui.theme.Paper
import kotlinx.coroutines.launch

@Composable
fun TrendingScreen(
    app: AppViewModel,
    onOpen: (SearchHit) -> Unit,
) {
    val state by app.trending.collectAsStateWithLifecycle()
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    var opening by remember { mutableStateOf<String?>(null) }

    LaunchedEffect(Unit) {
        app.refreshTrending()
    }

    val openTitle: (String) -> Unit = { title ->
        if (opening == null) {
            opening = title
            scope.launch {
                val hit = app.findTitle(title)
                opening = null
                if (hit != null) {
                    onOpen(hit)
                } else {
                    Toast.makeText(
                        context,
                        "No live source has \"$title\" yet",
                        Toast.LENGTH_SHORT,
                    ).show()
                }
            }
        }
    }

    LazyColumn(
        modifier = Modifier.fillMaxWidth().background(Night).statusBarsPadding(),
        contentPadding = PaddingValues(bottom = 30.dp),
    ) {
        item {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 20.dp)
                    .padding(top = 18.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.SpaceBetween,
            ) {
                Column {
                    Text(
                        "TRENDING",
                        color = Mint,
                        style = MaterialTheme.typography.labelMedium,
                        letterSpacing = 2.sp,
                    )
                    Text(
                        "AniTrendz",
                        color = Paper,
                        style = MaterialTheme.typography.headlineLarge,
                    )
                }
                IconButton(onClick = { app.refreshTrending(force = true) }) {
                    Icon(
                        Icons.Rounded.Refresh,
                        contentDescription = "Refresh trending",
                        tint = Ice,
                    )
                }
            }
        }
        when (val current = state) {
            Loadable.Idle, Loadable.Loading -> {
                items(6) {
                    TrendingSkeletonRow(Modifier.padding(horizontal = 20.dp, vertical = 7.dp))
                }
            }
            is Loadable.Failed -> {
                item {
                    Column(Modifier.padding(horizontal = 20.dp).padding(top = 18.dp)) {
                        ErrorPanel(
                            message = current.message,
                            onRetry = { app.refreshTrending(force = true) },
                        )
                    }
                }
            }
            is Loadable.Ready -> {
                val chart = current.value
                item {
                    ChartSummary(chart, Modifier.padding(horizontal = 20.dp, vertical = 18.dp))
                }
                if (chart.entries.isEmpty()) {
                    item {
                        Text(
                            "AniTrendz has not published this week's chart yet.",
                            modifier = Modifier.padding(horizontal = 20.dp, vertical = 24.dp),
                            color = Muted,
                            style = MaterialTheme.typography.bodyMedium,
                        )
                    }
                }
                itemsIndexed(
                    chart.entries,
                    key = { _, entry -> "${entry.rank}:${entry.uid ?: entry.title}" },
                ) { index, entry ->
                    TrendingRow(
                        entry = entry,
                        loading = opening == entry.title,
                        onClick = { openTitle(entry.title) },
                    )
                    if (index < chart.entries.lastIndex) {
                        Box(
                            Modifier
                                .fillMaxWidth()
                                .padding(start = 20.dp, end = 20.dp)
                                .height(1.dp)
                                .background(NightLine.copy(alpha = 0.35f)),
                        )
                    }
                }
                item {
                    Text(
                        text = buildString {
                            append("Live weekly ranking from AniTrendz")
                            if (chart.date.isNotBlank()) append(" · ${chart.date}")
                            if (chart.votes > 0) append(" · ${chart.votes} votes")
                        },
                        modifier = Modifier.padding(horizontal = 20.dp, vertical = 20.dp),
                        color = Muted.copy(alpha = 0.68f),
                        style = MaterialTheme.typography.bodySmall,
                    )
                }
            }
        }
    }
}

@Composable
private fun ChartSummary(chart: TrendingChart, modifier: Modifier = Modifier) {
    Surface(
        modifier = modifier.fillMaxWidth(),
        color = Color.Transparent,
        shape = RoundedCornerShape(24.dp),
    ) {
        Row(
            modifier = Modifier
                .background(
                    Brush.linearGradient(
                        listOf(Color(0xFF1D3157), Color(0xFF111722)),
                    ),
                )
                .padding(18.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.SpaceBetween,
        ) {
            Column(Modifier.weight(1f)) {
                Text(
                    "WEEK ${chart.week.ifBlank { "—" }} · ${chart.season.uppercase()}",
                    color = Ice,
                    style = MaterialTheme.typography.labelSmall,
                    letterSpacing = 1.3.sp,
                )
                Spacer(Modifier.height(4.dp))
                Text(
                    chart.title,
                    color = Paper,
                    style = MaterialTheme.typography.headlineMedium,
                )
                Text(
                    "Community ranking${if (chart.date.isNotBlank()) " · ${chart.date}" else ""}",
                    color = Muted,
                    style = MaterialTheme.typography.bodySmall,
                )
            }
            Column(horizontalAlignment = Alignment.End) {
                Text(
                    chart.entries.size.toString(),
                    color = Mint,
                    style = MaterialTheme.typography.headlineLarge,
                    fontWeight = FontWeight.Black,
                )
                Text(
                    "TITLES",
                    color = Muted,
                    style = MaterialTheme.typography.labelSmall,
                    letterSpacing = 1.sp,
                )
            }
        }
    }
}

@Composable
private fun TrendingRow(
    entry: TrendingEntry,
    loading: Boolean,
    onClick: () -> Unit,
) {
    val rankColor = when (entry.rank) {
        1 -> Color(0xFFFFD166)
        2 -> Color(0xFFD5DCE8)
        3 -> Color(0xFFD89B6C)
        else -> Muted
    }
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clickable(enabled = !loading, onClick = onClick)
            .padding(horizontal = 20.dp, vertical = 11.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Text(
            entry.rank.toString().padStart(2, '0'),
            modifier = Modifier.width(30.dp),
            color = rankColor,
            style = MaterialTheme.typography.titleLarge,
            fontWeight = FontWeight.Black,
        )
        Box(
            Modifier
                .size(width = 58.dp, height = 82.dp)
                .clip(RoundedCornerShape(12.dp)),
        ) {
            CoverImage(
                url = entry.cover,
                title = entry.title,
                modifier = Modifier.fillMaxWidth().height(82.dp),
            )
        }
        Column(Modifier.weight(1f)) {
            Text(
                entry.title,
                color = Paper,
                style = MaterialTheme.typography.titleMedium,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
            )
            if (!entry.studio.isNullOrBlank()) {
                Spacer(Modifier.height(2.dp))
                Text(
                    entry.studio,
                    color = Muted,
                    style = MaterialTheme.typography.bodySmall,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
            Spacer(Modifier.height(5.dp))
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(7.dp),
            ) {
                MovementChip(entry)
                if (!entry.tag.isNullOrBlank()) {
                    Text(
                        entry.tag,
                        color = Muted.copy(alpha = 0.75f),
                        style = MaterialTheme.typography.labelSmall,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
            }
        }
        if (loading) {
            CircularProgressIndicator(
                modifier = Modifier.size(18.dp),
                color = Ice,
                strokeWidth = 2.dp,
            )
        }
    }
}

@Composable
private fun MovementChip(entry: TrendingEntry) {
    val movement = entry.movement
    val (label, color, icon) = when {
        entry.isNew -> Triple("NEW", Ice, null)
        movement == null -> Triple("—", Muted, null)
        movement > 0 -> Triple("$movement", Mint, Icons.Rounded.ArrowUpward)
        movement < 0 -> Triple("${-movement}", Amber, Icons.Rounded.ArrowDownward)
        else -> Triple("—", Muted, null)
    }
    Row(
        modifier = Modifier
            .clip(CircleShape)
            .background(color.copy(alpha = 0.1f))
            .padding(horizontal = 7.dp, vertical = 3.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(2.dp),
    ) {
        if (icon != null) {
            Icon(
                icon,
                contentDescription = null,
                tint = color,
                modifier = Modifier.size(11.dp),
            )
        }
        Text(
            label,
            color = color,
            style = MaterialTheme.typography.labelSmall,
            fontWeight = FontWeight.Bold,
        )
    }
}

@Composable
private fun TrendingSkeletonRow(modifier: Modifier = Modifier) {
    Row(
        modifier = modifier.fillMaxWidth(),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        SkeletonBox(modifier = Modifier.size(width = 58.dp, height = 82.dp), corner = 12)
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(7.dp)) {
            Box(
                Modifier
                    .fillMaxWidth(0.72f)
                    .height(15.dp)
                    .clip(RoundedCornerShape(5.dp))
                    .background(NightCard),
            )
            Box(
                Modifier
                    .fillMaxWidth(0.45f)
                    .height(11.dp)
                    .clip(RoundedCornerShape(5.dp))
                    .background(NightCard.copy(alpha = 0.7f)),
            )
        }
    }
}