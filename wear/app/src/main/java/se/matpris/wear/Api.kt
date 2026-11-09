package se.matpris.wear

import android.annotation.SuppressLint
import android.content.Context
import android.content.pm.PackageManager
import android.location.Location
import com.google.android.gms.location.LocationServices
import com.google.android.gms.location.Priority
import com.google.android.gms.tasks.CancellationTokenSource
import com.google.android.gms.wearable.CapabilityClient
import com.google.android.gms.wearable.MessageClient
import com.google.android.gms.wearable.Wearable
import kotlinx.coroutines.FlowPreview
import kotlinx.coroutines.channels.awaitClose
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.callbackFlow
import kotlinx.coroutines.flow.timeout
import kotlinx.coroutines.tasks.await
import kotlinx.coroutines.withTimeoutOrNull
import org.json.JSONObject
import java.util.UUID
import kotlin.time.Duration.Companion.seconds

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

/**
 * The watch asks the Matpris app on the paired Android phone, which runs the search
 * with its Rust backend and answers over the Wear OS Data Layer (Bluetooth or Wi-Fi):
 *
 * - The phone app declares the capability [CAPABILITY] (res/values/wear.xml).
 * - The watch sends [SEARCH_PATH] with `{"id","q","lat","lng"}` (UTF-8 JSON).
 * - The phone answers with one message per search event on `"$EVENT_PATH/<id>"`, its
 *   payload one SearchEvent as JSON (the same events the page gets), ending with
 *   `{"type":"done"}` or `{"type":"error","message":…}`.
 *
 * Keep this in step with the phone side (src-tauri/gen/android, WearSearchService.kt).
 */
const val CAPABILITY = "matpris_search"
const val SEARCH_PATH = "/matpris/search"
const val EVENT_PATH = "/matpris/event"

/** No phone with Matpris is connected. */
class NoPhoneException : Exception("Install Matpris on your phone, and keep it nearby.")

/** The search's events as they arrive, so the first chains show while others work. */
@OptIn(FlowPreview::class)
fun search(context: Context, query: String, place: Place): Flow<SearchEvent> = callbackFlow {
    val id = UUID.randomUUID().toString()
    val messages = Wearable.getMessageClient(context)
    val listener = MessageClient.OnMessageReceivedListener { message ->
        if (message.path != "$EVENT_PATH/$id") return@OnMessageReceivedListener
        val event = runCatching { parseEvent(String(message.data, Charsets.UTF_8)) }.getOrNull()
            ?: return@OnMessageReceivedListener
        trySend(event)
        if (event is SearchEvent.Done || event is SearchEvent.Failed) close()
    }
    messages.addListener(listener).await()

    val nodes = Wearable.getCapabilityClient(context)
        .getCapability(CAPABILITY, CapabilityClient.FILTER_REACHABLE).await().nodes
    val phone = nodes.firstOrNull { it.isNearby } ?: nodes.firstOrNull()
    if (phone == null) {
        close(NoPhoneException())
    } else {
        val request = JSONObject()
            .put("id", id).put("q", query).put("lat", place.lat).put("lng", place.lng)
        messages.sendMessage(phone.id, SEARCH_PATH, request.toString().toByteArray(Charsets.UTF_8)).await()
    }
    awaitClose { messages.removeListener(listener) }
}.timeout(90.seconds)
