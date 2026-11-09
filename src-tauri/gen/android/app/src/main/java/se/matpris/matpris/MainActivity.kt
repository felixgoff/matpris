package se.matpris.matpris

import android.graphics.Color
import android.graphics.drawable.ColorDrawable
import android.graphics.drawable.LayerDrawable
import android.os.Bundle
import android.view.Gravity
import android.view.View
import androidx.activity.SystemBarStyle
import androidx.activity.enableEdgeToEdge
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    // Light icons on the steel rail at the top, dark ones on the page's ground below.
    enableEdgeToEdge(
      statusBarStyle = SystemBarStyle.dark(Color.TRANSPARENT),
      navigationBarStyle = SystemBarStyle.light(Color.TRANSPARENT, Color.TRANSPARENT),
    )
    super.onCreate(savedInstanceState)
    keepClearOfSystemBars()
  }

  /**
   * Android 15+ draws every app edge to edge, and the webview doesn't reliably tell the page
   * about the bars (env(safe-area-inset-*) can read 0), so the page would sit under the
   * status bar and the gesture bar. Instead the webview's parent is padded by the bars (and
   * the keyboard, which edge to edge no longer resizes the window for), and the padding is
   * painted in the page's own colours: the rail's steel on top, the ground below.
   */
  private fun keepClearOfSystemBars() {
    val content = findViewById<View>(android.R.id.content)
    val background = LayerDrawable(arrayOf(ColorDrawable(GROUND), ColorDrawable(STEEL)))
    background.setLayerGravity(1, Gravity.TOP or Gravity.FILL_HORIZONTAL)
    background.setLayerHeight(1, 0)
    content.background = background
    ViewCompat.setOnApplyWindowInsetsListener(content) { view, insets ->
      val bars = insets.getInsets(WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout())
      val keyboard = insets.getInsets(WindowInsetsCompat.Type.ime())
      background.setLayerHeight(1, bars.top)
      view.setPadding(bars.left, bars.top, bars.right, maxOf(bars.bottom, keyboard.bottom))
      WindowInsetsCompat.CONSUMED
    }
  }

  private companion object {
    val STEEL = Color.parseColor("#26292b")
    val GROUND = Color.parseColor("#e6e8e4")
  }
}
