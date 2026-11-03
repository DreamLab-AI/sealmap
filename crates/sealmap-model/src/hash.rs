use std::fmt;

use serde::{Deserialize, Serialize};

/// A stable, portable content hash: BLAKE3 over text with `\r\n` and lone
/// `\r` normalised to `\n`.
///
/// Displayed and serialised as `blake3:<64 hex chars>`, so the algorithm is
/// self-describing if it ever changes.
///
/// ```
/// use sealmap_model::ContentHash;
///
/// let unix = ContentHash::of_text("fn main() {}\n");
/// let windows = ContentHash::of_text("fn main() {}\r\n");
/// assert_eq!(unix, windows);
/// assert!(unix.to_string().starts_with("blake3:"));
/// ```
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct ContentHash(String);

impl<'de> Deserialize<'de> for ContentHash {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::parse(&text).ok_or_else(|| serde::de::Error::custom("expected blake3:<64 hex digits>"))
    }
}

impl ContentHash {
    /// Hash `text` after normalising line endings.
    pub fn of_text(text: &str) -> Self {
        let normalised = normalise_newlines(text);
        Self::of_bytes(normalised.as_bytes())
    }

    /// Hash raw bytes with no normalisation.
    pub fn of_bytes(bytes: &[u8]) -> Self {
        Self(format!("blake3:{}", blake3::hash(bytes).to_hex()))
    }

    /// Parse a previously displayed hash (`blake3:<hex>`).
    ///
    /// Returns `None` if the prefix or the hex payload is malformed.
    pub fn parse(s: &str) -> Option<Self> {
        let hex = s.strip_prefix("blake3:")?;
        (hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit())).then(|| Self(s.to_ascii_lowercase()))
    }

    /// The full `blake3:<hex>` string.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The first `n` hex characters of the digest, handy for short stable ids.
    pub fn short(&self, n: usize) -> &str {
        let hex = &self.0["blake3:".len()..];
        &hex[..n.min(hex.len())]
    }
}

impl fmt::Display for ContentHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A per-symbol content fingerprint: the first 16 bytes of a BLAKE3 digest.
///
/// Language adapters compute two per symbol (see `sealmap-extract`'s `fingerprint`
/// module): `sig_hash` over the contract and `body_hash` over the
/// implementation. The model only stores, compares, prints and parses them.
///
/// Displayed and serialised as `blake3-16:<32 hex chars>`. 128 bits keeps an
/// accidental collision out of reach for any realistic number of symbols
/// while halving the text a full digest would cost in every model and lock.
///
/// [`Fingerprint::default`] is all zeros and means "not fingerprinted": a
/// symbol built by hand with [`Symbol::new`](crate::Symbol::new) carries it
/// until a language adapter fills in real values.
///
/// ```
/// use sealmap_model::Fingerprint;
///
/// let f = Fingerprint::from_bytes([0xab; 16]);
/// let text = f.to_string();
/// assert_eq!(text, format!("blake3-16:{}", "ab".repeat(16)));
/// assert_eq!(Fingerprint::parse(&text), Some(f));
/// assert!(Fingerprint::default().is_unset());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Fingerprint([u8; 16]);

impl Fingerprint {
    /// The text prefix naming the algorithm and length.
    pub const PREFIX: &'static str = "blake3-16:";

    /// Wrap 16 digest bytes.
    pub const fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    /// Truncate a full BLAKE3 digest.
    pub fn from_blake3(hash: &blake3::Hash) -> Self {
        let mut b = [0u8; 16];
        b.copy_from_slice(&hash.as_bytes()[..16]);
        Self(b)
    }

    /// The digest bytes.
    pub fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }

    /// `true` for the all-zero "not fingerprinted" value.
    pub fn is_unset(&self) -> bool {
        self.0 == [0; 16]
    }

    /// Parse `blake3-16:<32 hex>` (either case).
    pub fn parse(s: &str) -> Option<Self> {
        let hex = s.strip_prefix(Self::PREFIX)?;
        if hex.len() != 32 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let mut b = [0u8; 16];
        for (i, out) in b.iter_mut().enumerate() {
            *out = u8::from_str_radix(hex.get(2 * i..2 * i + 2)?, 16).ok()?;
        }
        Some(Self(b))
    }

    /// Fold two fingerprints of the same symbol into one, independent of
    /// their order. Used when `#[cfg]`-gated twins share an id and are
    /// merged by [`Codebase::add_symbol`](crate::Codebase::add_symbol): a
    /// change to either twin changes the merged value.
    ///
    /// ```
    /// use sealmap_model::Fingerprint;
    ///
    /// let (a, b) = (Fingerprint::from_bytes([1; 16]), Fingerprint::from_bytes([2; 16]));
    /// assert_eq!(a.merge(b), b.merge(a));
    /// assert_ne!(a.merge(b), a);
    /// ```
    pub fn merge(self, other: Self) -> Self {
        let (lo, hi) = if self <= other { (self, other) } else { (other, self) };
        let mut h = blake3::Hasher::new_derive_key("sealmap sm1 2026-10 merged twin fingerprints");
        h.update(&lo.0);
        h.update(&hi.0);
        Self::from_blake3(&h.finalize())
    }
}

impl fmt::Display for Fingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(Self::PREFIX)?;
        for b in &self.0 {
            write!(f, "{b:02x}")?;
        }
        Ok(())
    }
}

impl Serialize for Fingerprint {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Fingerprint {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Self::parse(&s).ok_or_else(|| serde::de::Error::custom(format!("invalid fingerprint `{s}`")))
    }
}

/// Convert `\r\n` and lone `\r` to `\n`.
pub(crate) fn normalise_newlines(text: &str) -> std::borrow::Cow<'_, str> {
    if !text.contains('\r') {
        return std::borrow::Cow::Borrowed(text);
    }
    std::borrow::Cow::Owned(text.replace("\r\n", "\n").replace('\r', "\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_rejects_signed_byte_pairs_and_accepts_hex() {
        for pair in ["+0", "+a", "-0", " g"] {
            let text = format!("blake3-16:{}", pair.repeat(16));
            assert!(Fingerprint::parse(&text).is_none(), "{text}");
        }
        for pair in ["00", "af", "AF"] {
            let text = format!("blake3-16:{}", pair.repeat(16));
            assert!(Fingerprint::parse(&text).is_some(), "{text}");
        }
    }

    #[test]
    fn parse_round_trips() {
        let h = ContentHash::of_text("x");
        assert_eq!(ContentHash::parse(h.as_str()), Some(h.clone()));
        assert_eq!(ContentHash::parse("md5:abc"), None);
        assert_eq!(h.short(8).len(), 8);
    }

    #[test]
    fn deserialisation_preserves_the_hash_invariant() {
        for bad in ["", "x", "md5:abc", "blake3:abc", "blake3:🦀"] {
            let json = serde_json::to_string(bad).unwrap();
            assert!(serde_json::from_str::<ContentHash>(&json).is_err(), "{bad:?}");
        }
        let hash = ContentHash::of_text("receipt");
        let parsed: ContentHash = serde_json::from_str(&serde_json::to_string(&hash).unwrap()).unwrap();
        assert_eq!(parsed.short(999), &hash.as_str()[7..]);
    }
}
