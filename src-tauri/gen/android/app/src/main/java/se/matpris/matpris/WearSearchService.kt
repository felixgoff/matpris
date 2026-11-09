package se.matpris.matpris

import android.util.Log
import com.google.android.gms.tasks.Tasks
import com.google.android.gms.wearable.MessageEvent
import com.google.android.gms.wearable.Wearable
import com.google.android.gms.wearable.WearableListenerService
import org.json.JSONObject
import java.util.concurrent.TimeUnit

/**
 * Searches for the Galaxy Watch (wear/, Api.kt has the protocol): Play services starts
 * this when the watch sends [WatchRequest.SEARCH_PATH], also when the app isn't open,
 * and each search event goes back to the watch as one message on the request's path.
 *
 * The search runs on the thread Play services calls us on (not the main thread) and
 * blocks it until done: the service stays bound while it works, and messages leave in
 * order. A second search from the watch waits for the first, which ends within a minute
 * (DEADLINE in watch.rs), and the watch only listens for its latest anyway.
 */
class WearSearchService : WearableListenerService() {
    override fun onMessageReceived(event: MessageEvent) {
        if (event.path != WatchRequest.SEARCH_PATH) return
        val request = WatchRequest.parse(event.data)
        if (request == null) {
            Log.w(TAG, "a watch request we can't answer")
            return
        }
        val messages = Wearable.getMessageClient(this)
        val send = { json: String ->
            runCatching {
                val sent = messages.sendMessage(event.sourceNodeId, request.eventPath, json.toByteArray(Charsets.UTF_8))
                Tasks.await(sent, SEND_TIMEOUT_S, TimeUnit.SECONDS)
            }.onFailure { Log.w(TAG, "the watch can't be reached: $it") }.isSuccess
        }
        try {
            WatchBridge.search(request.q, request.lat, request.lng) { json -> send(json) }
        } catch (e: Throwable) {
            // E.g. the Rust library missing from a broken build: still tell the watch.
            Log.e(TAG, "the search failed", e)
            send(JSONObject().put("type", "error").put("message", "Matpris on your phone couldn't search").toString())
        }
    }

    private companion object {
        const val TAG = "MatprisWatch"
        const val SEND_TIMEOUT_S = 15L
    }
}
