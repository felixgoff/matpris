package se.matpris.matpris

import org.json.JSONObject

/**
 * What the watch asks (wear/, Api.kt): `{"id","q","lat","lng"}`. The search's own checks
 * (query length, a location in Sweden or else Lund) happen in Rust, like the page's;
 * here only what this side needs: an id that can be part of the answer's path.
 */
data class WatchRequest(val id: String, val q: String, val lat: Double, val lng: Double) {
    /** Where the answer's messages go. */
    val eventPath: String get() = "$EVENT_PATH/$id"

    companion object {
        const val SEARCH_PATH = "/matpris/search"
        const val EVENT_PATH = "/matpris/event"

        /** The watch sends a UUID; anything else can't be trusted in a path. */
        private val ID = Regex("[A-Za-z0-9-]{1,64}")

        /** The request, or null when it isn't one we can answer (no usable id). */
        fun parse(data: ByteArray): WatchRequest? {
            val json = runCatching { JSONObject(String(data, Charsets.UTF_8)) }.getOrNull() ?: return null
            val id = json.optString("id")
            if (!ID.matches(id)) return null
            // A missing or broken location becomes NaN, which Rust turns into Lund.
            return WatchRequest(id, json.optString("q"), json.optDouble("lat"), json.optDouble("lng"))
        }
    }
}
