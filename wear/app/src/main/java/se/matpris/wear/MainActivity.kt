package se.matpris.wear

import android.Manifest
import android.app.RemoteInput
import android.content.Context
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.compose.setContent
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.core.splashscreen.SplashScreen.Companion.installSplashScreen
import androidx.wear.compose.foundation.lazy.ScalingLazyColumn
import androidx.wear.compose.foundation.lazy.items
import androidx.wear.compose.foundation.lazy.rememberScalingLazyListState
import androidx.wear.compose.material3.AppScaffold
import androidx.wear.compose.material3.Button
import androidx.wear.compose.material3.ButtonDefaults
import androidx.wear.compose.material3.Card
import androidx.wear.compose.material3.CardDefaults
import androidx.wear.compose.material3.CircularProgressIndicator
import androidx.wear.compose.material3.ColorScheme
import androidx.wear.compose.material3.FilledTonalButton
import androidx.wear.compose.material3.ListHeader
import androidx.wear.compose.material3.MaterialTheme
import androidx.wear.compose.material3.ScreenScaffold
import androidx.wear.compose.material3.Text
import androidx.wear.compose.material3.TimeText
import androidx.wear.compose.navigation.SwipeDismissableNavHost
import androidx.wear.compose.navigation.composable
import androidx.wear.compose.navigation.rememberSwipeDismissableNavController
import androidx.wear.input.RemoteInputIntentHelper
import kotlinx.coroutines.flow.catch
import java.net.URLDecoder
import java.net.URLEncoder

// Matpris on the wrist (Galaxy Watch 5 and other Wear OS 3+ watches): say or type a
// product and see what's cheapest near you, ranked like the web page's answer card.

private val Yellow = Color(0xFFFFD400)
private val Ink = Color(0xFF1A1C1D)

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        installSplashScreen()
        super.onCreate(savedInstanceState)
        setContent { MatprisTheme { MatprisApp() } }
    }
}

@Composable
internal fun MatprisTheme(content: @Composable () -> Unit) {
    MaterialTheme(
        colorScheme = ColorScheme(primary = Yellow, onPrimary = Ink, primaryContainer = Yellow, onPrimaryContainer = Ink),
        content = content
    )
}

@Composable
private fun MatprisApp() {
    val nav = rememberSwipeDismissableNavController()
    AppScaffold(timeText = { TimeText() }) {
        SwipeDismissableNavHost(navController = nav, startDestination = "home") {
            composable("home") {
                Home(onSearch = { q -> nav.navigate("results/" + URLEncoder.encode(q, "UTF-8")) })
            }
            composable("results/{q}") { entry ->
                Results(URLDecoder.decode(entry.arguments?.getString("q").orEmpty(), "UTF-8"))
            }
        }
    }
}

private const val QUERY_KEY = "query"
private val STARTERS = listOf("mjölk", "kaffe", "bananer", "ägg", "smör")

/** The last searches, newest first, filled up with common groceries. */
private fun recentSearches(context: Context): List<String> {
    val saved = context.getSharedPreferences("matpris", Context.MODE_PRIVATE).getString("recent", "").orEmpty()
    return (saved.split('\n').filter { it.isNotBlank() } + STARTERS).distinct().take(6)
}

private fun saveSearch(context: Context, query: String) {
    val list = (listOf(query) + recentSearches(context)).distinct().take(6)
    context.getSharedPreferences("matpris", Context.MODE_PRIVATE).edit().putString("recent", list.joinToString("\n")).apply()
}

@Composable
internal fun Home(onSearch: (String) -> Unit) {
    val context = LocalContext.current
    val listState = rememberScalingLazyListState()
    val recent = remember { recentSearches(context) }
    val askPermission = rememberLauncherForActivityResult(ActivityResultContracts.RequestMultiplePermissions()) {}
    LaunchedEffect(Unit) {
        askPermission.launch(arrayOf(Manifest.permission.ACCESS_FINE_LOCATION, Manifest.permission.ACCESS_COARSE_LOCATION))
    }
    val start = { q: String ->
        val query = q.trim()
        if (query.length >= 2) {
            saveSearch(context, query)
            onSearch(query)
        }
    }
    // Voice or keyboard, whichever the shopper picks on the watch's input screen.
    val input = rememberLauncherForActivityResult(ActivityResultContracts.StartActivityForResult()) { result ->
        val text = result.data?.let { RemoteInput.getResultsFromIntent(it)?.getCharSequence(QUERY_KEY) }
        if (text != null) start(text.toString())
    }

    ScreenScaffold(scrollState = listState) {
        ScalingLazyColumn(state = listState, modifier = Modifier.fillMaxWidth()) {
            item { Wordmark() }
            item {
                Button(
                    onClick = {
                        val intent = RemoteInputIntentHelper.createActionRemoteInputIntent()
                        RemoteInputIntentHelper.putRemoteInputsExtra(
                            intent,
                            listOf(RemoteInput.Builder(QUERY_KEY).setLabel("Search groceries").build())
                        )
                        input.launch(intent)
                    },
                    modifier = Modifier.fillMaxWidth(),
                    label = { Text("Search") },
                    secondaryLabel = { Text("Say or type a product") }
                )
            }
            item { ListHeader { Text("Recent") } }
            items(recent) { q ->
                FilledTonalButton(onClick = { start(q) }, modifier = Modifier.fillMaxWidth(), label = { Text(q) })
            }
        }
    }
}

@Composable
private fun Wordmark() {
    Row(modifier = Modifier.padding(bottom = 4.dp)) {
        Text("MAT", fontWeight = FontWeight.Black, fontSize = 22.sp)
        Text("PRIS", fontWeight = FontWeight.Black, fontSize = 22.sp, color = Yellow)
    }
}

@Composable
private fun Results(query: String) {
    val context = LocalContext.current
    val rows = remember { mutableStateListOf<Row>() }
    val statuses = remember { mutableStateMapOf<Chain, String>() }
    var place by remember { mutableStateOf<Place?>(null) }
    var done by remember { mutableStateOf(false) }
    var failure by remember { mutableStateOf<String?>(null) }

    LaunchedEffect(query) {
        val here = currentPlace(context)
        place = here
        search(context, query, here)
            .catch { e ->
                failure = when (e) {
                    is NoPhoneException -> e.message
                    is kotlinx.coroutines.TimeoutCancellationException -> "Your phone didn't answer in time."
                    else -> "Your phone couldn't search right now."
                }
            }
            .collect { e ->
                when (e) {
                    is SearchEvent.ChainResult -> {
                        statuses[e.chain] = e.status
                        rows.addAll(e.rows)
                    }
                    SearchEvent.Done -> done = true
                    is SearchEvent.Failed -> failure = e.message
                    SearchEvent.Other -> {}
                }
            }
        done = true
    }

    ResultsList(query, rows, statuses, place, done, failure)
}

/** The results screen for what has arrived so far (also drawn by the screenshot tests). */
@Composable
internal fun ResultsList(
    query: String,
    rows: List<Row>,
    statuses: Map<Chain, String>,
    place: Place?,
    done: Boolean,
    failure: String?,
) {
    val listState = rememberScalingLazyListState()
    val ranking = Ranking(rows.toList())
    val cheapest = ranking.cheapest
    val others = ranking.ranked.filter { it !== cheapest }.take(12)

    ScreenScaffold(scrollState = listState) {
        ScalingLazyColumn(state = listState, modifier = Modifier.fillMaxWidth()) {
            item { ListHeader { Text(query, maxLines = 1, overflow = TextOverflow.Ellipsis) } }
            when {
                failure != null -> item { Text(failure!!, textAlign = TextAlign.Center) }
                cheapest == null && !done -> item {
                    Column(horizontalAlignment = Alignment.CenterHorizontally) {
                        CircularProgressIndicator(modifier = Modifier.size(32.dp))
                        Spacer(Modifier.padding(4.dp))
                        Text(
                            if (place == null) "Finding you…" else "Asking the stores…",
                            style = MaterialTheme.typography.bodySmall
                        )
                    }
                }
                cheapest == null -> item { Text("No prices for “$query” near you.", textAlign = TextAlign.Center) }
                else -> {
                    item { CheapestCard(cheapest, ranking.figure(cheapest)) }
                    if (others.isNotEmpty()) item { ListHeader { Text("Also near you") } }
                    items(others, key = { "${it.chain}:${it.id}" }) { r -> RowCard(r, ranking.figure(r)) }
                }
            }
            if (cheapest != null || done) item { Footer(statuses, done, place) }
        }
    }
}

@Composable
private fun CheapestCard(r: Row, f: Figure) {
    Card(
        onClick = {},
        modifier = Modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(containerColor = Yellow, contentColor = Ink)
    ) {
        Text("CHEAPEST NEAR YOU", fontSize = 10.sp, fontWeight = FontWeight.Bold, color = Ink.copy(alpha = 0.7f))
        Text(r.name, fontWeight = FontWeight.Bold, maxLines = 2, overflow = TextOverflow.Ellipsis, color = Ink)
        Text(f.big, fontSize = 24.sp, fontWeight = FontWeight.Black, color = Ink)
        f.small?.let { Text(it, fontSize = 11.sp, color = Ink) }
        StoreLine(r, Ink)
    }
}

@Composable
private fun RowCard(r: Row, f: Figure) {
    Card(onClick = {}, modifier = Modifier.fillMaxWidth()) {
        Text(r.name, maxLines = 2, overflow = TextOverflow.Ellipsis, style = MaterialTheme.typography.labelMedium)
        Text(f.big, fontWeight = FontWeight.Bold, fontSize = 16.sp)
        f.small?.let { Text(it, style = MaterialTheme.typography.bodySmall) }
        StoreLine(r, MaterialTheme.colorScheme.onSurfaceVariant)
    }
}

@Composable
private fun StoreLine(r: Row, color: Color) {
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.Start) {
        Box(Modifier.size(8.dp).background(Color(r.chain.color), CircleShape))
        Spacer(Modifier.width(4.dp))
        Text(
            "${r.chain.label} ${r.shortStoreName} · ${km(r.distanceKm)}",
            fontSize = 11.sp,
            color = color,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis
        )
    }
}

@Composable
private fun Footer(statuses: Map<Chain, String>, done: Boolean, place: Place?) {
    val priced = statuses.count { it.value == "ok" }
    val busy = statuses.filter { it.value == "throttled" || it.value == "error" }.keys.joinToString { it.label }
    val text = buildString {
        append(if (done) "Prices from $priced chains" else "Prices from $priced chains so far…")
        if (busy.isNotEmpty()) append(". Busy: $busy")
        if (place?.fromDevice == false) append(". Location off: near Lund")
    }
    Text(text, textAlign = TextAlign.Center, style = MaterialTheme.typography.bodySmall, modifier = Modifier.padding(top = 8.dp))
}
