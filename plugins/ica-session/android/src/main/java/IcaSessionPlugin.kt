package se.matpris.icasession

import android.app.Activity
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.view.View
import android.view.ViewGroup
import android.webkit.CookieManager
import android.webkit.WebView
import android.webkit.WebViewClient
import android.widget.FrameLayout
import androidx.appcompat.app.AlertDialog
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSArray
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin

@InvokeArg
class MintArgs {
    lateinit var url: String
    lateinit var cookie: String
    var quietMs: Long = 20_000
    var personMs: Long = 180_000
    lateinit var title: String
}

/**
 * ICA's WAF cookie, minted the way a shopper's browser gets it: the shop opens in a
 * webview of its own, out of sight, and once the WAF has set its cookie the shop's
 * cookies go back to Rust (src-tauri/src/ica.rs). If ICA wants a person instead, the
 * webview is shown in a dialog for the person using the phone; nothing here answers the
 * check.
 */
@TauriPlugin
class IcaSessionPlugin(private val activity: Activity) : Plugin(activity) {
    /** One visit at a time (Rust asks one at a time too). */
    private var visit: Visit? = null

    @Command
    fun mint(invoke: Invoke) {
        val args = invoke.parseArgs(MintArgs::class.java)
        activity.runOnUiThread {
            // The watch's searches (WearSearchService) can run on after the app's window
            // has closed: with no activity to show ICA's check in, ICA is just busy.
            if (activity.isFinishing || activity.isDestroyed) {
                invoke.reject("Open Matpris on the phone for ICA's prices")
                return@runOnUiThread
            }
            if (visit != null) {
                invoke.reject("ICA's shop is already open")
                return@runOnUiThread
            }
            visit = Visit(activity, args) { cookies, error ->
                visit = null
                if (cookies == null) {
                    invoke.reject(error)
                } else {
                    val list = JSArray()
                    for ((name, value) in cookies) list.put(JSObject().put("name", name).put("value", value))
                    invoke.resolve(JSObject().put("cookies", list))
                }
            }.also { it.start() }
        }
    }
}

/** One visit to the shop. Everything here runs on the main thread, as WebView requires. */
private class Visit(
    private val activity: Activity,
    private val args: MintArgs,
    private val done: (cookies: List<Pair<String, String>>?, error: String?) -> Unit,
) {
    private val main = Handler(Looper.getMainLooper())
    private val jar = CookieManager.getInstance()
    private var view: WebView? = null
    private var dialog: AlertDialog? = null
    private var deadline = 0L
    private var finished = false
    private val tick = Runnable { poll() }

    fun start() {
        jar.setAcceptCookie(true)
        forgetToken {
            val v = WebView(activity)
            v.settings.javaScriptEnabled = true
            v.settings.domStorageEnabled = true
            // Without a client, a redirect would leave for the system browser.
            v.webViewClient = WebViewClient()
            // Attached behind the app's own webview, so the shop's scripts run as they do on
            // screen (a webview outside the window may not run them), but nobody sees it.
            v.alpha = 0f
            v.importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO_HIDE_DESCENDANTS
            content().addView(v, 0, FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT))
            view = v
            v.loadUrl(args.url)
            deadline = SystemClock.uptimeMillis() + args.quietMs
            main.postDelayed(tick, POLL_MS)
        }
    }

    private fun content(): ViewGroup = activity.findViewById(android.R.id.content)

    /**
     * Expires any token left from an earlier visit (the webview keeps its cookies across
     * launches), so the one we wait for is new. The WAF may have set it for the shop's
     * host or for ica.se; a removal that matches nothing does nothing.
     */
    private fun forgetToken(then: () -> Unit) {
        val removals = listOf("", "; Domain=${host()}", "; Domain=.ica.se")
        var left = removals.size
        for (domain in removals) {
            jar.setCookie(args.url, "${args.cookie}=; Max-Age=0; Path=/$domain") {
                left -= 1
                if (left == 0) then()
            }
        }
    }

    private fun host(): String = android.net.Uri.parse(args.url).host ?: ""

    /** The cookies the webview would send to the shop, once the token is among them. */
    private fun cookies(): List<Pair<String, String>>? {
        val header = jar.getCookie(args.url) ?: return null
        val all = header.split(";").mapNotNull {
            val pair = it.trim()
            val eq = pair.indexOf('=')
            if (eq <= 0) null else pair.substring(0, eq) to pair.substring(eq + 1)
        }
        return if (all.any { it.first == args.cookie && it.second.isNotEmpty() }) all else null
    }

    private fun poll() {
        if (finished) return
        cookies()?.let { return finish(it, null) }
        if (SystemClock.uptimeMillis() >= deadline) {
            if (dialog != null) return finish(null, "ICA wants a person to answer its check first")
            askPerson()
        }
        main.postDelayed(tick, POLL_MS)
    }

    /** ICA's check is for a person: show it to the one using the phone, and wait. */
    private fun askPerson() {
        val v = view ?: return
        // A dialog on an activity that is going away throws (BadTokenException) and would
        // take the app down with it; there is nobody to ask then anyway.
        if (activity.isFinishing || activity.isDestroyed) {
            return finish(null, "ICA wants a person to answer its check first")
        }
        content().removeView(v)
        v.alpha = 1f
        v.importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_AUTO
        val box = FrameLayout(activity)
        val height = (activity.resources.displayMetrics.heightPixels * 0.7).toInt()
        box.addView(v, FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, height))
        dialog = AlertDialog.Builder(activity)
            .setTitle(args.title)
            .setView(box)
            .setNegativeButton("Not now") { _, _ -> finish(null, "ICA's check was closed") }
            .setOnCancelListener { finish(null, "ICA's check was closed") }
            .create()
            .also { it.show() }
        deadline = SystemClock.uptimeMillis() + args.personMs
    }

    private fun finish(cookies: List<Pair<String, String>>?, error: String?) {
        if (finished) return
        finished = true
        main.removeCallbacks(tick)
        dialog?.setOnCancelListener(null)
        dialog?.dismiss()
        view?.let {
            (it.parent as? ViewGroup)?.removeView(it)
            it.stopLoading()
            it.destroy()
        }
        view = null
        jar.flush()
        done(cookies, error)
    }

    companion object {
        const val POLL_MS = 500L
    }
}
