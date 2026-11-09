package se.matpris.wear

import android.annotation.SuppressLint
import android.content.Context
import android.content.pm.PackageManager
import android.location.Location
import com.google.android.gms.location.LocationServices
import com.google.android.gms.location.Priority
import com.google.android.gms.tasks.CancellationTokenSource
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.tasks.await
import kotlinx.coroutines.withTimeoutOrNull
import java.net.HttpURLConnection
import java.net.URL
import java.net.URLEncoder

data class Place(val lat: Double, val lng: Double, val fromDevice: Boolean)

/** Where the owner tests from, and the web app's default: Lund. */
val LUND = Place(55.7047, 13.1910, fromDevice = false)

/** The watch's position, or Lund without permission or a fix within 15 s. */
@SuppressLint("MissingPermission")
suspend fun currentPlace(context: Context): Place {
    val granted = listOf(android.Manifest.permission.ACCESS_FINE_LOCATION, android.Manifest.permission.ACCESS_COARSE_LOCATION)
        .any { context.checkSelfPermission(it) == PackageManager.PERMISSION_GRANTED }
    if (!granted) return LUND
    val client = LocationServices.getFusedLocationProviderClient(context)
    val fix: Location? = runCatching {
        withTimeoutOrNull(15_000) {
            client.getCurrentLocation(Priority.PRIORITY_BALANCED_POWER_ACCURACY, CancellationTokenSource().token).await()
        } ?: client.lastLocation.await()
    }.getOrNull()
    return fix?.let { Place(it.latitude, it.longitude, fromDevice = true) } ?: LUND
}

/** /api/search's events as they stream in, so the first chains show while others work. */
fun search(query: String, place: Place): Flow<SearchEvent> = flow {
    val q = URLEncoder.encode(query, "UTF-8")
    val url = URL("${BuildConfig.SERVER_URL}/api/search?q=$q&lat=${place.lat}&lng=${place.lng}")
    val conn = url.openConnection() as HttpURLConnection
    conn.connectTimeout = 15_000
    // Axfood's crawl delay can hold a chain back for ~10 s; ICA can take longer.
    conn.readTimeout = 90_000
    conn.setRequestProperty("accept", "application/x-ndjson")
    try {
        if (conn.responseCode != 200) throw IllegalStateException("The server answered ${conn.responseCode}")
        conn.inputStream.bufferedReader().useLines { lines ->
            for (line in lines) if (line.isNotBlank()) emit(parseEvent(line))
        }
    } finally {
        conn.disconnect()
    }
}.flowOn(Dispatchers.IO)
