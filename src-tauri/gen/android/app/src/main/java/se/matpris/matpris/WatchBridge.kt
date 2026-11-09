package se.matpris.matpris

/**
 * The Rust side of the watch bridge (src-tauri/src/watch.rs): the same search the page
 * runs, in this process, whether or not the app's window is open.
 */
object WatchBridge {
    init {
        // The Tauri library ([lib] name = "matpris_lib" in src-tauri/Cargo.toml). Loading
        // it again when the activity already has is a no-op.
        System.loadLibrary("matpris_lib")
    }

    fun interface Events {
        /** One search event as JSON for the watch; false stops the search. */
        fun onEvent(json: String): Boolean
    }

    /**
     * Blocks until the search is done; [events] hears every message on this thread, in
     * order, the last one `done` or `error`. Never call it on the main thread.
     */
    @JvmStatic
    external fun search(q: String, lat: Double, lng: Double, events: Events)
}
