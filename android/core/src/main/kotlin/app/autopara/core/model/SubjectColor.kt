package app.autopara.core.model

import java.security.MessageDigest

/**
 * The colour a subject always wears. Same seven pastels and the same hash as the desktop app, so a
 * subject keeps its colour on every device. Port of `ui/class_card.subject_color`.
 */
object SubjectColor {
    /** Coral, lemon, sky, violet, mint, pink and dusty blue, as 0xRRGGBB. */
    val PALETTE = intArrayOf(0xec6b66, 0xf1f36a, 0x86cdf7, 0x8566e6, 0x8fe8a8, 0xf26c9c, 0xa8bdd6)

    fun of(subject: String): Int {
        val digest = MessageDigest.getInstance("MD5").digest(subject.trim().lowercase().toByteArray(Charsets.UTF_8))
        // The first eight hex digits are the first four bytes, read big-endian.
        var value = 0L
        for (i in 0 until 4) value = (value shl 8) or (digest[i].toLong() and 0xff)
        return PALETTE[(value % PALETTE.size).toInt()]
    }
}
