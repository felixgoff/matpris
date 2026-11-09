package se.matpris.matpris

import android.util.Log
import com.google.android.gms.tasks.Tasks
import com.google.android.gms.wearable.MessageEvent
import com.google.android.gms.wearable.Wearable
import com.google.android.gms.wearable.WearableListenerService
import org.json.JSONObject
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit

/**
 * Searches for the Galaxy Watch (wear/, Api.kt has the protocol): Play services starts
 * this when the watch sends [WatchRequest.SEARCH_PATH], also when the app isn't open,
 * and each search event goes back to the watch as one message on the request's path.
 *
 * Play services delivers messages one at a time, so the search runs on its own thread
 * and this returns at once: a newer search from the same watch can then arrive and
 * replace it. The older one stops at its next event (its sender answers false), since
 * the watch only listens for its latest; each still ends within a minute (DEADLINE in
 * watch.rs).
 */
class WearSearchService : WearableListenerService() {
    override fun onMessageReceived(event: MessageEvent) {
        if (event.path != WatchRequest.SEARCH_PATH) return
        val request = WatchRequest.parse(event.data)
        if (request == null) {
            Log.w(TAG, "a watch request we can't answer")
            return
        }
        val watch = event.sourceNodeId
        latest[watch] = request.id
        val messages = Wearable.getMessageClient(this)
        val send = send@{ json: String ->
            if (latest[watch] != request.id) return@send false
            runCatching {
                val sent = messages.sendMessage(event.sourceNodeId, request.eventPath, json.toByteArray(Charsets.UTF_8))
                Tasks.await(sent, SEND_TIMEOUT_S, TimeUnit.SECONDS)
            }.onFailure { Log.w(TAG, "the watch can't be reached: $it") }.isSuccess
        }
        searches.execute {
            try {
                WatchBridge.search(request.q, request.lat, request.lng) { json -> send(json) }
            } catch (e: Throwable) {
                // E.g. the Rust library missing from a broken build: still tell the watch.
                Log.e(TAG, "the search failed", e)
                send(JSONObject().put("type", "error").put("message", "Matpris on your phone couldn't search").toString())
            }
        }
    }

    private companion object {
        /** Per watch, the search it is waiting for; any older one stops. */
        val latest = ConcurrentHashMap<String, String>()
        /** A replaced search and its successor may overlap briefly (one stops at its next event). */
        val searches = Executors.newCachedThreadPool()
        const val TAG = "MatprisWatch"
        const val SEND_TIMEOUT_S = 15L
    }
}
