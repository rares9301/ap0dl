package com.ap0dl.anyomi.ui.screens

import android.widget.Toast
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
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
import com.ap0dl.anyomi.data.ScheduleEntry
import com.ap0dl.anyomi.data.SearchHit
import com.ap0dl.anyomi.ui.components.CoverImage
import com.ap0dl.anyomi.ui.components.EmptyPanel
import com.ap0dl.anyomi.ui.components.ErrorPanel
import com.ap0dl.anyomi.ui.components.SkeletonBox
import com.ap0dl.anyomi.ui.theme.Ice
import com.ap0dl.anyomi.ui.theme.Mint
import com.ap0dl.anyomi.ui.theme.Muted
import com.ap0dl.anyomi.ui.theme.Night
import com.ap0dl.anyomi.ui.theme.NightCard
import com.ap0dl.anyomi.ui.theme.NightLine
import com.ap0dl.anyomi.ui.theme.Paper
import java.time.DayOfWeek
import java.time.Instant
import java.time.LocalDate
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.time.format.TextStyle
import java.time.temporal.TemporalAdjusters
import java.util.Locale
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

@Composable
fun ScheduleScreen(
    app: AppViewModel,
    onOpen: (SearchHit) -> Unit,
) {
    val zone = remember { ZoneId.systemDefault() }
    val today = remember { LocalDate.now(zone) }
    val weekStart = remember(today) {
        today.with(TemporalAdjusters.previousOrSame(DayOfWeek.MONDAY))
    }
    val weekEnd = remember(weekStart) { weekStart.plusDays(6) }
    val startEpoch = remember(weekStart, zone) {
        weekStart.atStartOfDay(zone).toEpochSecond()
    }
    val endEpoch = remember(weekStart, zone) {
        weekStart.plusDays(7).atStartOfDay(zone).toEpochSecond()
    }
    val state by app.schedule.collectAsStateWithLifecycle()
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    var selectedDay by remember(weekStart) {
        mutableStateOf(if (!today.isBefore(weekStart) && today.isBefore(weekStart.plusDays(7))) today else weekStart)
    }
    var opening by remember { mutableStateOf<String?>(null) }
    var now by remember { mutableStateOf(Instant.now()) }

    LaunchedEffect(weekStart) {
        val ready = state as? Loadable.Ready
        app.refreshSchedule(
            start = startEpoch,
            end = endEpoch,
            force = ready != null && ready.value.start != startEpoch,
        )
    }
    LaunchedEffect(Unit) {
        while (true) {
            now = Instant.now()
            delay(60_000)
        }
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

    val entries = (state as? Loadable.Ready)?.value?.entries.orEmpty()
    val dayEntries = remember(entries, selectedDay, zone) {
        entries.filter {
            Instant.ofEpochSecond(it.airingAt).atZone(zone).toLocalDate() == selectedDay
        }.sortedBy { it.airingAt }
    }
    val dayCounts = remember(entries, weekStart, zone) {
        (0..6).associateWith { offset ->
            val day = weekStart.plusDays(offset.toLong())
            entries.count {
                Instant.ofEpochSecond(it.airingAt).atZone(zone).toLocalDate() == day
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
                        "SCHEDULE",
                        color = Mint,
                        style = MaterialTheme.typography.labelMedium,
                        letterSpacing = 2.sp,
                    )
                    Text(
                        "This week",
                        color = Paper,
                        style = MaterialTheme.typography.headlineLarge,
                    )
                }
                IconButton(onClick = {
                    app.refreshSchedule(startEpoch, endEpoch, force = true)
                }) {
                    Icon(
                        Icons.Rounded.Refresh,
                        contentDescription = "Refresh schedule",
                        tint = Ice,
                    )
                }
            }
        }
        item {
            WeekSummary(
                weekStart = weekStart,
                weekEnd = weekEnd,
                total = entries.size,
                modifier = Modifier.padding(horizontal = 20.dp, vertical = 18.dp),
            )
        }
        item {
            Row(
                modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp),
                horizontalArrangement = Arrangement.spacedBy(4.dp),
            ) {
                (0..6).forEach { offset ->
                    val day = weekStart.plusDays(offset.toLong())
                    DayCell(
                        day = day,
                        count = dayCounts[offset] ?: 0,
                        selected = day == selectedDay,
                        onClick = { selectedDay = day },
                        modifier = Modifier.weight(1f),
                    )
                }
            }
        }
        when (val current = state) {
            Loadable.Idle, Loadable.Loading -> {
                items(5) {
                    ScheduleSkeleton(Modifier.padding(horizontal = 20.dp, vertical = 7.dp))
                }
            }
            is Loadable.Failed -> {
                item {
                    Column(Modifier.padding(horizontal = 20.dp).padding(top = 18.dp)) {
                        ErrorPanel(
                            message = current.message,
                            onRetry = {
                                app.refreshSchedule(startEpoch, endEpoch, force = true)
                            },
                        )
                    }
                }
            }
            is Loadable.Ready -> {
                if (dayEntries.isEmpty()) {
                    item {
                        EmptyPanel(
                            title = "Nothing scheduled",
                            message = "No releases land on this day in your timezone.",
                            modifier = Modifier.padding(horizontal = 20.dp),
                        )
                    }
                } else {
                    items(dayEntries, key = { "${it.id}:${it.episode}:${it.airingAt}" }) { entry ->
                        ScheduleRow(
                            entry = entry,
                            zone = zone,
                            now = now,
                            loading = opening == entry.title,
                            onClick = { openTitle(entry.title) },
                        )
                    }
                }
                item {
                    Text(
                        "Airing times use ${zone.id} · data from AniList",
                        modifier = Modifier.padding(horizontal = 20.dp, vertical = 18.dp),
                        color = Muted.copy(alpha = 0.68f),
                        style = MaterialTheme.typography.bodySmall,
                    )
                }
            }
        }
    }
}

private fun weekLabel(start: LocalDate, end: LocalDate): String {
    val locale = Locale.getDefault()
    val startMonth = start.month.getDisplayName(TextStyle.SHORT, locale).uppercase(locale)
    val endMonth = end.month.getDisplayName(TextStyle.SHORT, locale).uppercase(locale)
    return if (startMonth == endMonth) {
        "$startMonth ${start.dayOfMonth} — $startMonth ${end.dayOfMonth}"
    } else {
        "$startMonth ${start.dayOfMonth} — $endMonth ${end.dayOfMonth}"
    }
}

@Composable
private fun WeekSummary(
    weekStart: LocalDate,
    weekEnd: LocalDate,
    total: Int,
    modifier: Modifier = Modifier,
) {
    Row(
        modifier = modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(24.dp))
            .background(
                Brush.linearGradient(
                    listOf(Color(0xFF203B53), Color(0xFF111722)),
                ),
            )
            .padding(18.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.SpaceBetween,
    ) {
        Column(Modifier.weight(1f)) {
            Text(
                weekLabel(weekStart, weekEnd),
                color = Ice,
                style = MaterialTheme.typography.labelSmall,
                letterSpacing = 1.3.sp,
            )
            Spacer(Modifier.height(4.dp))
            Text(
                "Weekly releases",
                color = Paper,
                style = MaterialTheme.typography.headlineMedium,
            )
            Text(
                "Simulcast airings in your local time",
                color = Muted,
                style = MaterialTheme.typography.bodySmall,
            )
        }
        Column(horizontalAlignment = Alignment.End) {
            Text(
                total.toString(),
                color = Mint,
                style = MaterialTheme.typography.headlineLarge,
                fontWeight = FontWeight.Black,
            )
            Text(
                "EPISODES",
                color = Muted,
                style = MaterialTheme.typography.labelSmall,
                letterSpacing = 1.sp,
            )
        }
    }
}

@Composable
private fun DayCell(
    day: LocalDate,
    count: Int,
    selected: Boolean,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Surface(
        modifier = modifier
            .clip(RoundedCornerShape(14.dp))
            .clickable(onClick = onClick),
        color = if (selected) Ice else NightCard.copy(alpha = 0.72f),
        shape = RoundedCornerShape(14.dp),
        border = androidx.compose.foundation.BorderStroke(
            1.dp,
            if (selected) Ice else NightLine.copy(alpha = 0.55f),
        ),
    ) {
        Column(
            modifier = Modifier.padding(vertical = 9.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Text(
                day.dayOfWeek.getDisplayName(TextStyle.SHORT, Locale.getDefault()).uppercase(),
                color = if (selected) Night.copy(alpha = 0.68f) else Muted,
                style = MaterialTheme.typography.labelSmall,
                letterSpacing = 0.5.sp,
            )
            Spacer(Modifier.height(2.dp))
            Text(
                day.dayOfMonth.toString(),
                color = if (selected) Night else Paper,
                style = MaterialTheme.typography.titleMedium,
                fontWeight = FontWeight.Bold,
            )
            Spacer(Modifier.height(4.dp))
            Row(horizontalArrangement = Arrangement.spacedBy(2.dp)) {
                repeat(count.coerceAtMost(3)) {
                    Box(
                        Modifier
                            .size(3.dp)
                            .clip(CircleShape)
                            .background(if (selected) Night.copy(alpha = 0.55f) else Mint),
                    )
                }
            }
        }
    }
}

@Composable
private fun ScheduleRow(
    entry: ScheduleEntry,
    zone: ZoneId,
    now: Instant,
    loading: Boolean,
    onClick: () -> Unit,
) {
    val airing = Instant.ofEpochSecond(entry.airingAt).atZone(zone)
    val time = remember(entry.airingAt, zone) {
        airing.format(DateTimeFormatter.ofPattern("HH:mm"))
    }
    val (countdown, countdownColor) = remember(entry.airingAt, now) {
        countdownLabel(entry.airingAt, now)
    }
    val isAiring = countdown == "Airing now"

    Row(
        modifier = Modifier
            .fillMaxWidth()
            .height(IntrinsicSize.Min)
            .padding(horizontal = 20.dp),
        verticalAlignment = Alignment.Top,
    ) {
        Column(
            modifier = Modifier.width(47.dp).padding(top = 15.dp),
            horizontalAlignment = Alignment.End,
        ) {
            Text(
                time,
                color = Paper,
                style = MaterialTheme.typography.titleMedium,
                fontWeight = FontWeight.Bold,
            )
            Text(
                "EP ${entry.episode}",
                color = Muted,
                style = MaterialTheme.typography.labelSmall,
            )
        }
        Box(Modifier.width(20.dp).fillMaxHeight()) {
            Box(
                Modifier
                    .align(Alignment.Center)
                    .width(1.dp)
                    .fillMaxHeight()
                    .background(NightLine.copy(alpha = 0.55f)),
            )
            Box(
                Modifier
                    .align(Alignment.TopCenter)
                    .padding(top = 20.dp)
                    .size(7.dp)
                    .clip(CircleShape)
                    .background(if (isAiring) Mint else Ice),
            )
        }
        Surface(
            modifier = Modifier
                .weight(1f)
                .padding(vertical = 6.dp)
                .clip(RoundedCornerShape(16.dp))
                .clickable(enabled = !loading, onClick = onClick),
            color = NightCard.copy(alpha = 0.78f),
            shape = RoundedCornerShape(16.dp),
            border = androidx.compose.foundation.BorderStroke(1.dp, NightLine.copy(alpha = 0.55f)),
        ) {
            Row(
                modifier = Modifier.padding(10.dp),
                horizontalArrangement = Arrangement.spacedBy(11.dp),
            ) {
                Box(
                    Modifier
                        .size(width = 52.dp, height = 74.dp)
                        .clip(RoundedCornerShape(10.dp)),
                ) {
                    CoverImage(
                        url = entry.cover,
                        title = entry.title,
                        modifier = Modifier.fillMaxWidth().height(74.dp),
                    )
                }
                Column(Modifier.weight(1f)) {
                    Text(
                        entry.title,
                        color = Paper,
                        style = MaterialTheme.typography.titleSmall,
                        maxLines = 2,
                        overflow = TextOverflow.Ellipsis,
                    )
                    Spacer(Modifier.height(3.dp))
                    Text(
                        listOfNotNull(entry.format, entry.genres.firstOrNull())
                            .joinToString(" · ")
                            .ifBlank { "TV" },
                        color = Muted,
                        style = MaterialTheme.typography.labelSmall,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                    Spacer(Modifier.height(7.dp))
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(6.dp),
                    ) {
                        Text(
                            countdown,
                            modifier = Modifier
                                .clip(RoundedCornerShape(50))
                                .background(countdownColor.copy(alpha = 0.11f))
                                .padding(horizontal = 7.dp, vertical = 3.dp),
                            color = countdownColor,
                            style = MaterialTheme.typography.labelSmall,
                            fontWeight = FontWeight.SemiBold,
                        )
                        if (loading) {
                            CircularProgressIndicator(
                                modifier = Modifier.size(13.dp),
                                color = Ice,
                                strokeWidth = 2.dp,
                            )
                        }
                    }
                }
            }
        }
    }
}

private fun countdownLabel(airingAt: Long, now: Instant): Pair<String, Color> {
    val seconds = airingAt - now.epochSecond
    return when {
        seconds in -7_200..0 -> "Airing now" to Mint
        seconds > 0 -> {
            val minutes = (seconds / 60).toInt()
            val hours = minutes / 60
            val days = hours / 24
            when {
                days > 0 -> "in ${days}d ${hours % 24}h" to Ice
                hours > 0 -> "in ${hours}h ${minutes % 60}m" to Ice
                else -> "in ${minutes.coerceAtLeast(1)}m" to Ice
            }
        }
        else -> "Aired" to Muted
    }
}

@Composable
private fun ScheduleSkeleton(modifier: Modifier = Modifier) {
    Row(
        modifier = modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        SkeletonBox(modifier = Modifier.width(47.dp).height(15.dp), corner = 5)
        SkeletonBox(modifier = Modifier.width(58.dp).height(82.dp), corner = 12)
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(7.dp)) {
            SkeletonBox(modifier = Modifier.fillMaxWidth(0.8f).height(15.dp), corner = 5)
            SkeletonBox(modifier = Modifier.fillMaxWidth(0.55f).height(11.dp), corner = 5)
        }
    }
}