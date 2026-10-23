package com.ap0dl.anyomi

import android.graphics.Color
import android.os.Bundle
import android.widget.Toast
import androidx.activity.ComponentActivity
import androidx.activity.SystemBarStyle
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.core.splashscreen.SplashScreen.Companion.installSplashScreen
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Download
import androidx.compose.material.icons.rounded.Explore
import androidx.compose.material.icons.rounded.Home
import androidx.compose.material.icons.rounded.MoreHoriz
import androidx.compose.material3.Badge
import androidx.compose.material3.BadgedBox
import androidx.compose.material3.Icon
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.NavigationBarItemDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalContext
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import androidx.navigation.NavGraph.Companion.findStartDestination
import androidx.navigation.NavType
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.currentBackStackEntryAsState
import androidx.navigation.compose.rememberNavController
import androidx.navigation.navArgument
import com.ap0dl.anyomi.ui.screens.BrowseScreen
import com.ap0dl.anyomi.ui.screens.DetailsScreen
import com.ap0dl.anyomi.ui.screens.DownloadsScreen
import com.ap0dl.anyomi.ui.screens.HomeScreen
import com.ap0dl.anyomi.ui.screens.MoreScreen
import com.ap0dl.anyomi.ui.screens.PlayerScreen
import com.ap0dl.anyomi.ui.theme.AnyomiTheme
import com.ap0dl.anyomi.ui.theme.Ice
import com.ap0dl.anyomi.ui.theme.Muted
import com.ap0dl.anyomi.ui.theme.Night

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        installSplashScreen()
        enableEdgeToEdge(
            statusBarStyle = SystemBarStyle.dark(Color.TRANSPARENT),
            navigationBarStyle = SystemBarStyle.dark(Color.TRANSPARENT),
        )
        super.onCreate(savedInstanceState)
        setContent {
            AnyomiTheme {
                AnyomiApp()
            }
        }
    }
}

private data class Destination(
    val route: String,
    val label: String,
    val icon: ImageVector,
)

private val destinations = listOf(
    Destination("home", "Home", Icons.Rounded.Home),
    Destination("browse?provider=all", "Browse", Icons.Rounded.Explore),
    Destination("downloads", "Downloads", Icons.Rounded.Download),
    Destination("more", "More", Icons.Rounded.MoreHoriz),
)

@Composable
private fun AnyomiApp() {
    val nav = rememberNavController()
    val app: AppViewModel = viewModel()
    val backStack by nav.currentBackStackEntryAsState()
    val route = backStack?.destination?.route.orEmpty()
    val showBottomBar = route == "home" || route.startsWith("browse") || route == "downloads" || route == "more"
    val playRequest by app.playRequest.collectAsStateWithLifecycle()
    val activeDownloads by app.activeDownloads.collectAsStateWithLifecycle()
    val context = LocalContext.current

    Column(Modifier.fillMaxSize().background(Night)) {
        Box(Modifier.weight(1f)) {
        NavHost(
            navController = nav,
            startDestination = "home",
            modifier = Modifier.fillMaxSize(),
        ) {
            composable("home") {
                HomeScreen(
                    app = app,
                    onOpen = { hit ->
                        app.select(hit)
                        nav.navigate("details") { launchSingleTop = true }
                    },
                    onBrowse = { nav.navigate("browse") },
                    onBrowseSource = { provider ->
                        nav.navigate("browse?provider=$provider")
                    },
                    onResume = { item ->
                        app.play(item.anime, item.episode, localPath = item.localPath)
                        nav.navigate("player")
                    },
                )
            }
            composable(
                route = "browse?provider={provider}",
                arguments = listOf(
                    navArgument("provider") {
                        type = NavType.StringType
                        defaultValue = "all"
                    },
                ),
            ) { entry ->
                BrowseScreen(
                    app = app,
                    navProvider = entry.arguments?.getString("provider")?.takeIf { it != "all" },
                    onOpen = { hit ->
                        app.select(hit)
                        nav.navigate("details") { launchSingleTop = true }
                    },
                )
            }
            composable("details") {
                val hit = app.selectedHit.collectAsStateWithLifecycle().value
                if (hit == null) {
                    nav.popBackStack()
                } else {
                    DetailsScreen(
                        app = app,
                        hit = hit,
                        onBack = { nav.popBackStack() },
                        onPlay = { anime, episode, episodes ->
                            app.play(anime, episode, episodes)
                            nav.navigate("player")
                        },
                        onDownload = { anime, episode ->
                            val audio = if (app.audioPreference.value == "dub") "Dub" else "Sub"
                            app.startDownload(anime, episode)
                            Toast.makeText(
                                context,
                                "Download queued · trying $audio audio",
                                Toast.LENGTH_SHORT,
                            ).show()
                        },
                    )
                }
            }
            composable("player") {
                val request = playRequest
                if (request == null) {
                    androidx.compose.runtime.LaunchedEffect(Unit) { nav.popBackStack() }
                } else {
                    PlayerScreen(
                        app = app,
                        request = request,
                        onBack = { nav.popBackStack() },
                    )
                }
            }
            composable("downloads") {
                DownloadsScreen(
                    app = app,
                    onBrowse = { nav.navigate("browse") },
                    onPlayLocal = { download ->
                        app.playLocal(download)
                        nav.navigate("player")
                    },
                )
            }
            composable("more") {
                MoreScreen(app = app)
            }
        }
        }
        if (showBottomBar) {
            NavigationBar(
                containerColor = Night,
                contentColor = Muted,
                tonalElevation = androidx.compose.ui.unit.Dp.Unspecified,
            ) {
                destinations.forEach { destination ->
                    val selected = when {
                        destination.route.startsWith("home") -> route == "home"
                        destination.route.startsWith("browse") -> route.startsWith("browse")
                        else -> route == destination.route
                    }
                    NavigationBarItem(
                        selected = selected,
                        onClick = {
                            val target = if (destination.route == "browse?provider=all") "browse" else destination.route
                            nav.navigate(target) {
                                popUpTo(nav.graph.findStartDestination().id) {
                                    saveState = true
                                }
                                launchSingleTop = true
                                restoreState = true
                            }
                        },
                        icon = {
                            if (destination.route == "downloads" && activeDownloads.any { it.active }) {
                                BadgedBox(
                                    badge = {
                                        Badge { Text(activeDownloads.count { it.active }.toString()) }
                                    },
                                ) {
                                    Icon(destination.icon, contentDescription = destination.label)
                                }
                            } else {
                                Icon(destination.icon, contentDescription = destination.label)
                            }
                        },
                        label = { Text(destination.label) },
                        colors = NavigationBarItemDefaults.colors(
                            selectedIconColor = Ice,
                            selectedTextColor = Ice,
                            indicatorColor = Ice.copy(alpha = 0.13f),
                            unselectedIconColor = Muted,
                            unselectedTextColor = Muted,
                        ),
                    )
                }
            }
        }
    }
}