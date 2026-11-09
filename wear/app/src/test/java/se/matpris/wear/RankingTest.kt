package se.matpris.wear

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class RankingTest {
    private fun row(
        id: String,
        price: Double,
        unitPrice: Double? = null,
        unit: String? = "l",
        match: String = "best",
        deal: Deal? = null,
        chain: Chain = Chain.willys,
    ) = Row(chain, id, "Product $id", null, null, price, unitPrice, unit, false, null, null, deal, "${chain.label} Lund", 1.0, match)

    @Test fun cheapestByComparisonPriceWithinTheMainUnit() {
        val r = Ranking(listOf(row("a", 20.0, 20.0), row("b", 30.0, 15.0), row("c", 5.0, 2.0, unit = "kg")))
        assertEquals("b", r.cheapest?.id)
        assertEquals(listOf("b", "a", "c"), r.ranked.map { it.id })
    }

    @Test fun onlyTheTopTierIsRanked() {
        val r = Ranking(listOf(row("yoghurt", 5.0, 5.0, match = "mention"), row("milk", 15.0, 15.0)))
        assertEquals(listOf("milk"), r.ranked.map { it.id })
    }

    @Test fun aCheaperMultiBuyDealCounts() {
        val deal = Deal(quantity = 3, each = 10.0, total = 30.0, unitPrice = 10.0)
        val r = Ranking(listOf(row("a", 14.0, 14.0, deal = deal), row("b", 12.0, 12.0)))
        assertEquals("a", r.cheapest?.id)
        assertEquals(Figure("10.00 kr/l", "10.00 kr each when you buy 3"), r.figure(r.cheapest!!))
    }

    @Test fun noRowsNoAnswer() {
        assertNull(Ranking(emptyList()).cheapest)
    }

    @Test fun parsesAChainEvent() {
        val line = """{"type":"chain","chain":"coop","status":"ok","results":[{"chain":"coop","id":"1","name":"Mellanmjölk","price":17.9,"unitPrice":11.93,"unit":"l","url":"u","pricing":"chain","match":"best","store":{"id":"s","chain":"coop","name":"Coop Mårtenstorget","lat":0,"lng":0,"address":"a","distanceKm":0.5}}]}"""
        val e = parseEvent(line) as SearchEvent.ChainResult
        assertEquals(Chain.coop, e.chain)
        assertEquals("Mårtenstorget", e.rows.single().shortStoreName)
        assertEquals("1,234.50 kr", kr(1234.5))
    }
}
