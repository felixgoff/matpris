package se.matpris.wear

import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onRoot
import com.github.takahirom.roborazzi.captureRoboImage
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

/** Draws the watch screens on a round 450 px screen, like the Galaxy Watch 5 (44 mm):
 *  ./gradlew recordRoborazziDebug, images in app/build/outputs/roborazzi. */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [33], qualifiers = "w227dp-h227dp-round-xhdpi")
class ScreensTest {
    @get:Rule val compose = createComposeRule()

    private fun row(id: String, name: String, chain: Chain, price: Double, unitPrice: Double, store: String, km: Double, deal: Deal? = null) =
        Row(chain, id, name, null, "1.5 l", price, unitPrice, "l", false, null, null, deal, store, km, "best")

    private val rows = listOf(
        row("1", "Mellanmjölk 1,5%", Chain.willys, 17.90, 11.93, "Willys Hemma Lund Stortorget", 0.26),
        row("2", "Mellanmjölk Färsk 1,5%", Chain.coop, 18.50, 12.33, "Coop Mårtenstorget", 0.5),
        row("3", "Mellanmjölk", Chain.lidl, 16.49, 10.99, "Lidl Lund, Åkerlund och Rausings väg 1", 1.02, Deal(2, 15.0, 30.0, 10.0)),
        row("4", "Arla Ko Mellanmjölk", Chain.hemkop, 19.90, 13.27, "Hemköp Lund Karhögstorg", 1.13),
    )
    private val statuses = mapOf(Chain.willys to "ok", Chain.coop to "ok", Chain.lidl to "ok", Chain.hemkop to "ok", Chain.ica to "throttled")

    @Test fun results() {
        compose.setContent { MatprisTheme { ResultsList("mjölk", rows, statuses, LUND, true, null) } }
        compose.onRoot().captureRoboImage("build/outputs/roborazzi/results.png")
    }

    @Test fun loading() {
        compose.setContent { MatprisTheme { ResultsList("mjölk", emptyList(), emptyMap(), null, false, null) } }
        compose.onRoot().captureRoboImage("build/outputs/roborazzi/loading.png")
    }

    @Test fun home() {
        compose.setContent { MatprisTheme { Home(onSearch = {}) } }
        compose.onRoot().captureRoboImage("build/outputs/roborazzi/home.png")
    }
}
