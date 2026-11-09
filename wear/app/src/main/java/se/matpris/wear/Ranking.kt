package se.matpris.wear

import org.json.JSONObject
import java.text.DecimalFormat
import java.text.DecimalFormatSymbols
import java.util.Locale

// The search results and their ranking, as the web page does it (src/routes/+page.svelte)
// with its defaults: sorted by comparison price, multi-buy deals counted.

enum class Chain(val label: String, val color: Long) {
    willys("Willys", 0xFFE30613),
    hemkop("Hemköp", 0xFFF07800),
    lidl("Lidl", 0xFF0050AA),
    citygross("City Gross", 0xFF0171B9),
    ica("ICA", 0xFFE3000B),
    coop("Coop", 0xFF00AA46);

    companion object {
        fun of(id: String) = entries.firstOrNull { it.name == id }
    }
}

data class Deal(val quantity: Int, val each: Double, val total: Double, val unitPrice: Double?)

data class Row(
    val chain: Chain,
    val id: String,
    val name: String,
    val brand: String?,
    val size: String?,
    val price: Double,
    val unitPrice: Double?,
    /** "kg", "l" or "st". */
    val unit: String?,
    val perKg: Boolean,
    val approxEach: Double?,
    val oldPrice: Double?,
    val deal: Deal?,
    val storeName: String,
    val distanceKm: Double,
    /** "best", "mention" or "weak": see `relevance` in src/lib/server/normalize.ts. */
    val match: String,
) {
    /** "Willys Hemma Lund Stortorget" → "Hemma Lund Stortorget": the chain is named beside it. */
    val shortStoreName: String
        get() = storeName.removePrefix(chain.label).trim().ifEmpty { storeName }
}

/** One line of /api/search's NDJSON stream. */
sealed interface SearchEvent {
    data class ChainResult(val chain: Chain, val status: String, val rows: List<Row>) : SearchEvent
    data object Done : SearchEvent
    data object Other : SearchEvent
}

private fun JSONObject.optNumber(key: String): Double? = if (has(key) && !isNull(key)) getDouble(key) else null
private fun JSONObject.optText(key: String): String? = if (has(key) && !isNull(key)) getString(key) else null

fun parseEvent(line: String): SearchEvent {
    val e = JSONObject(line)
    return when (e.optString("type")) {
        "chain" -> {
            val chain = Chain.of(e.getString("chain")) ?: return SearchEvent.Other
            val results = e.optJSONArray("results")
            val rows = (0 until (results?.length() ?: 0)).mapNotNull { i -> parseRow(results!!.getJSONObject(i)) }
            SearchEvent.ChainResult(chain, e.getString("status"), rows)
        }
        "done" -> SearchEvent.Done
        else -> SearchEvent.Other
    }
}

private fun parseRow(r: JSONObject): Row? {
    val chain = Chain.of(r.getString("chain")) ?: return null
    val store = r.getJSONObject("store")
    val deal = r.optJSONObject("deal")?.let {
        Deal(it.getInt("quantity"), it.getDouble("each"), it.getDouble("total"), it.optNumber("unitPrice"))
    }
    return Row(
        chain = chain,
        id = r.getString("id"),
        name = r.getString("name"),
        brand = r.optText("brand"),
        size = r.optText("size"),
        price = r.getDouble("price"),
        unitPrice = r.optNumber("unitPrice"),
        unit = r.optText("unit"),
        perKg = r.optBoolean("perKg", false),
        approxEach = r.optNumber("approxEach"),
        oldPrice = r.optNumber("oldPrice"),
        deal = deal,
        storeName = store.getString("name"),
        distanceKm = store.getDouble("distanceKm"),
        match = r.getString("match"),
    )
}

/** What a result card shows: the figure the list is ranked by, large, and the other price. */
data class Figure(val big: String, val small: String?)

class Ranking(rows: List<Row>) {
    /** Only the products that are the thing searched for are ranked. */
    private val tier = listOf("best", "mention", "weak").firstOrNull { t -> rows.any { it.match == t } }
    private val relevant = rows.filter { it.match == tier }

    /** Unit prices are compared within the unit most results use (kr/kg vs kr/l vs kr/pc). */
    private val mainUnit = relevant.mapNotNull { it.unit }.groupingBy { it }.eachCount().maxByOrNull { it.value }?.key

    private fun usesDeal(r: Row): Boolean {
        val dealUnit = r.deal?.unitPrice ?: return false
        return r.unitPrice == null || dealUnit < r.unitPrice
    }
    private fun rankUnitPrice(r: Row) = if (usesDeal(r)) r.deal!!.unitPrice else r.unitPrice
    private fun rankPrice(r: Row) = if (usesDeal(r)) r.deal!!.each else (r.approxEach ?: r.price)
    private fun loose(r: Row) = r.perKg && r.approxEach == null
    private fun comparable(r: Row) = r.unit == mainUnit && rankUnitPrice(r) != null

    /** Comparable rows first, by comparison price; the rest after them, by item price. */
    val ranked: List<Row> = relevant.sortedWith { a, b ->
        val ca = comparable(a)
        val cb = comparable(b)
        when {
            ca != cb -> if (ca) -1 else 1
            ca && rankUnitPrice(a) != rankUnitPrice(b) -> rankUnitPrice(a)!!.compareTo(rankUnitPrice(b)!!)
            else -> rankPrice(a).compareTo(rankPrice(b))
        }
    }

    val cheapest: Row? = ranked.firstOrNull { comparable(it) } ?: ranked.firstOrNull()

    fun figure(r: Row): Figure {
        val each = when {
            usesDeal(r) -> "${kr(r.deal!!.each)} each when you buy ${r.deal.quantity}"
            loose(r) -> "${kr(r.price)}/kg"
            r.approxEach != null -> "≈ ${kr(r.approxEach)} each"
            else -> kr(r.price)
        }
        val unitPrice = rankUnitPrice(r)
        if (comparable(r)) return Figure("${kr(unitPrice!!)}/${unitLabel(r.unit)}", each)
        val big = when {
            loose(r) -> "${kr(r.price)}/kg"
            usesDeal(r) -> kr(r.deal!!.each)
            r.approxEach != null -> "≈ ${kr(r.approxEach)}"
            else -> kr(r.price)
        }
        val small = when {
            usesDeal(r) -> "each when you buy ${r.deal!!.quantity}"
            unitPrice != null -> "${kr(unitPrice)}/${unitLabel(r.unit)}"
            else -> null
        }
        return Figure(big, small)
    }
}

private fun unitLabel(unit: String?) = when (unit) {
    "st" -> "pc"
    null -> "pc"
    else -> unit
}

private val money = DecimalFormat("#,##0.00", DecimalFormatSymbols(Locale.UK))
private val short = DecimalFormat("#,##0.#", DecimalFormatSymbols(Locale.UK))

/** "19.40 kr", like src/lib/format.ts. */
fun kr(n: Double): String = "${money.format(n)} kr"

/** "1.1 km". */
fun km(n: Double): String = "${short.format(n)} km"
