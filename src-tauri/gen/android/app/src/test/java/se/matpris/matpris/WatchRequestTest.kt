package se.matpris.matpris

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class WatchRequestTest {
    private fun parse(json: String) = WatchRequest.parse(json.toByteArray(Charsets.UTF_8))

    @Test
    fun readsWhatTheWatchSends() {
        val r = parse("""{"id":"3f2b9c1e-5d6a-4e2f-9a7b-0c1d2e3f4a5b","q":"fanta","lat":55.7047,"lng":13.191}""")!!
        assertEquals(WatchRequest("3f2b9c1e-5d6a-4e2f-9a7b-0c1d2e3f4a5b", "fanta", 55.7047, 13.191), r)
        assertEquals("/matpris/event/3f2b9c1e-5d6a-4e2f-9a7b-0c1d2e3f4a5b", r.eventPath)
    }

    @Test
    fun aBrokenLocationIsLeftForRustToReplaceWithLund() {
        val r = parse("""{"id":"abc","q":"kaffe","lat":"here"}""")!!
        assertTrue(r.lat.isNaN() && r.lng.isNaN())
    }

    @Test
    fun aRequestWithoutAUsableIdIsIgnored() {
        assertNull(parse("""{"q":"fanta","lat":55.7,"lng":13.2}"""))
        assertNull(parse("""{"id":"../../x","q":"fanta"}"""))
        assertNull(parse("""{"id":"${"a".repeat(65)}","q":"fanta"}"""))
        assertNull(parse("not json"))
    }

    @Test
    fun aShortQueryStillGoesToRustWhichAnswersWithAnError() {
        assertEquals("", parse("""{"id":"abc"}""")!!.q)
    }
}
