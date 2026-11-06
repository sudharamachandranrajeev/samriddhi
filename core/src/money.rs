//! Money, kept exactly. An amount is a whole number of the currency's minor
//! unit (pence, paise, fils) next to its ISO 4217 code; it is never a float.
//! Exchange rates are exact decimals, and converting rounds once, half to
//! even, to the minor unit of the target currency.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MoneyError {
    #[error("{0:?} is not a currency code we know (ISO 4217, such as GBP, INR or AED)")]
    UnknownCurrency(String),
    #[error("{0:?} is not an amount")]
    NotAnAmount(String),
    #[error("{currency} has only {exponent} digits after the point")]
    TooPrecise { currency: Currency, exponent: u8 },
    #[error("cannot mix {0} and {1} without an exchange rate")]
    Mismatch(Currency, Currency),
    #[error("the amount is too large")]
    Overflow,
    #[error("{0:?} is not a rate")]
    NotARate(String),
    #[error("a rate from {want} cannot convert {got}")]
    WrongRate { want: Currency, got: Currency },
}

/// Active ISO 4217 currencies and their minor-unit digits (exponent), plus
/// two withdrawn recently (BGN, ANG) that old statements still carry. Fund
/// codes and metals (XAU, CLF...) are left out: nobody is paid in them.
/// Sorted by code, so a lookup is a binary search.
#[rustfmt::skip]
const CURRENCIES: &[(&str, u8)] = &[
    ("AED", 2), ("AFN", 2), ("ALL", 2), ("AMD", 2), ("ANG", 2), ("AOA", 2), ("ARS", 2),
    ("AUD", 2), ("AWG", 2), ("AZN", 2), ("BAM", 2), ("BBD", 2), ("BDT", 2), ("BGN", 2),
    ("BHD", 3), ("BIF", 0), ("BMD", 2), ("BND", 2), ("BOB", 2), ("BRL", 2), ("BSD", 2),
    ("BTN", 2), ("BWP", 2), ("BYN", 2), ("BZD", 2), ("CAD", 2), ("CDF", 2), ("CHF", 2),
    ("CLP", 0), ("CNY", 2), ("COP", 2), ("CRC", 2), ("CUP", 2), ("CVE", 2), ("CZK", 2),
    ("DJF", 0), ("DKK", 2), ("DOP", 2), ("DZD", 2), ("EGP", 2), ("ERN", 2), ("ETB", 2),
    ("EUR", 2), ("FJD", 2), ("FKP", 2), ("GBP", 2), ("GEL", 2), ("GHS", 2), ("GIP", 2),
    ("GMD", 2), ("GNF", 0), ("GTQ", 2), ("GYD", 2), ("HKD", 2), ("HNL", 2), ("HTG", 2),
    ("HUF", 2), ("IDR", 2), ("ILS", 2), ("INR", 2), ("IQD", 3), ("IRR", 2), ("ISK", 0),
    ("JMD", 2), ("JOD", 3), ("JPY", 0), ("KES", 2), ("KGS", 2), ("KHR", 2), ("KMF", 0),
    ("KPW", 2), ("KRW", 0), ("KWD", 3), ("KYD", 2), ("KZT", 2), ("LAK", 2), ("LBP", 2),
    ("LKR", 2), ("LRD", 2), ("LSL", 2), ("LYD", 3), ("MAD", 2), ("MDL", 2), ("MGA", 2),
    ("MKD", 2), ("MMK", 2), ("MNT", 2), ("MOP", 2), ("MRU", 2), ("MUR", 2), ("MVR", 2),
    ("MWK", 2), ("MXN", 2), ("MYR", 2), ("MZN", 2), ("NAD", 2), ("NGN", 2), ("NIO", 2),
    ("NOK", 2), ("NPR", 2), ("NZD", 2), ("OMR", 3), ("PAB", 2), ("PEN", 2), ("PGK", 2),
    ("PHP", 2), ("PKR", 2), ("PLN", 2), ("PYG", 0), ("QAR", 2), ("RON", 2), ("RSD", 2),
    ("RUB", 2), ("RWF", 0), ("SAR", 2), ("SBD", 2), ("SCR", 2), ("SDG", 2), ("SEK", 2),
    ("SGD", 2), ("SHP", 2), ("SLE", 2), ("SOS", 2), ("SRD", 2), ("SSP", 2), ("STN", 2),
    ("SVC", 2), ("SYP", 2), ("SZL", 2), ("THB", 2), ("TJS", 2), ("TMT", 2), ("TND", 3),
    ("TOP", 2), ("TRY", 2), ("TTD", 2), ("TWD", 2), ("TZS", 2), ("UAH", 2), ("UGX", 0),
    ("USD", 2), ("UYU", 2), ("UYW", 4), ("UZS", 2), ("VED", 2), ("VES", 2), ("VND", 0),
    ("VUV", 0), ("WST", 2), ("XAF", 0), ("XCD", 2), ("XCG", 2), ("XOF", 0), ("XPF", 0),
    ("YER", 2), ("ZAR", 2), ("ZMW", 2), ("ZWG", 2),
];

/// An ISO 4217 currency from the table above.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Currency([u8; 3]);

impl Currency {
    pub const GBP: Currency = Currency(*b"GBP");
    pub const INR: Currency = Currency(*b"INR");
    pub const AED: Currency = Currency(*b"AED");
    pub const USD: Currency = Currency(*b"USD");
    pub const EUR: Currency = Currency(*b"EUR");

    /// A currency by its code, in any case ("gbp" works).
    pub fn new(code: &str) -> Result<Currency, MoneyError> {
        let up = code.trim().to_ascii_uppercase();
        let i = CURRENCIES
            .binary_search_by(|(c, _)| c.cmp(&up.as_str()))
            .map_err(|_| MoneyError::UnknownCurrency(code.to_owned()))?;
        let b = CURRENCIES[i].0.as_bytes();
        Ok(Currency([b[0], b[1], b[2]]))
    }

    pub fn code(&self) -> &str {
        // Only built from the table, so always ASCII.
        std::str::from_utf8(&self.0).unwrap_or("XXX")
    }

    /// Digits after the point: 2 for GBP, 0 for JPY, 3 for KWD.
    pub fn exponent(&self) -> u8 {
        CURRENCIES
            .binary_search_by(|(c, _)| c.cmp(&self.code()))
            .map(|i| CURRENCIES[i].1)
            .unwrap_or(2)
    }

    /// Every currency we know, by code.
    pub fn all() -> impl Iterator<Item = Currency> {
        CURRENCIES.iter().map(|(c, _)| {
            let b = c.as_bytes();
            Currency([b[0], b[1], b[2]])
        })
    }
}

impl fmt::Display for Currency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

impl FromStr for Currency {
    type Err = MoneyError;
    fn from_str(s: &str) -> Result<Currency, MoneyError> {
        Currency::new(s)
    }
}

impl Serialize for Currency {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.code())
    }
}

impl<'de> Deserialize<'de> for Currency {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Currency, D::Error> {
        let s = String::deserialize(d)?;
        Currency::new(&s).map_err(serde::de::Error::custom)
    }
}

/// An exact amount: `minor` units of `currency` (12345 GBP = £123.45).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Money {
    pub minor: i64,
    pub currency: Currency,
}

impl Money {
    pub fn new(minor: i64, currency: Currency) -> Money {
        Money { minor, currency }
    }

    pub fn zero(currency: Currency) -> Money {
        Money::new(0, currency)
    }

    /// Read an amount as typed: "1,234.56", "-12", "1,23,456.7" (Indian
    /// grouping), "1 234". More digits after the point than the currency has
    /// is refused rather than rounded: money typed in must stay exact.
    pub fn parse(text: &str, currency: Currency) -> Result<Money, MoneyError> {
        let bad = || MoneyError::NotAnAmount(text.to_owned());
        let t = text.trim();
        let (negative, t) = match t.strip_prefix('-') {
            Some(rest) => (true, rest.trim_start()),
            None => (false, t.strip_prefix('+').unwrap_or(t)),
        };
        let digits: String = t
            .chars()
            .filter(|c| !matches!(c, ',' | ' ' | '_'))
            .collect();
        let (whole, frac) = match digits.split_once('.') {
            Some((w, f)) => (w, f),
            None => (digits.as_str(), ""),
        };
        let all_digits = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
        if (whole.is_empty() && frac.is_empty()) || !all_digits(whole) || !all_digits(frac) {
            return Err(bad());
        }
        let exp = currency.exponent();
        if frac.len() > exp as usize {
            return Err(MoneyError::TooPrecise {
                currency,
                exponent: exp,
            });
        }
        let padded = format!("{whole}{frac:0<width$}", width = exp as usize);
        let minor: i64 = if padded.is_empty() {
            0
        } else {
            padded.parse().map_err(|_| MoneyError::Overflow)?
        };
        Ok(Money::new(if negative { -minor } else { minor }, currency))
    }

    /// The amount as a plain decimal with no grouping, e.g. "-1234.50"; the
    /// form used in exports.
    pub fn decimal(&self) -> String {
        let exp = self.currency.exponent() as u32;
        let sign = if self.minor < 0 { "-" } else { "" };
        let abs = self.minor.unsigned_abs();
        if exp == 0 {
            return format!("{sign}{abs}");
        }
        let unit = 10u64.pow(exp);
        format!(
            "{sign}{}.{:0width$}",
            abs / unit,
            abs % unit,
            width = exp as usize
        )
    }

    /// For people: thousands grouped, code after, e.g. "1,234.50 GBP".
    /// Screens may format by locale instead; this is the plain fallback.
    pub fn format(&self) -> String {
        let plain = self.decimal();
        let (sign, rest) = match plain.strip_prefix('-') {
            Some(r) => ("-", r),
            None => ("", plain.as_str()),
        };
        let (whole, frac) = match rest.split_once('.') {
            Some((w, f)) => (w, Some(f)),
            None => (rest, None),
        };
        let mut grouped = String::new();
        for (i, c) in whole.chars().enumerate() {
            if i > 0 && (whole.len() - i) % 3 == 0 {
                grouped.push(',');
            }
            grouped.push(c);
        }
        match frac {
            Some(f) => format!("{sign}{grouped}.{f} {}", self.currency),
            None => format!("{sign}{grouped} {}", self.currency),
        }
    }

    fn same(&self, other: &Money) -> Result<(), MoneyError> {
        if self.currency == other.currency {
            Ok(())
        } else {
            Err(MoneyError::Mismatch(self.currency, other.currency))
        }
    }

    pub fn checked_add(&self, other: Money) -> Result<Money, MoneyError> {
        self.same(&other)?;
        let minor = self
            .minor
            .checked_add(other.minor)
            .ok_or(MoneyError::Overflow)?;
        Ok(Money::new(minor, self.currency))
    }

    pub fn checked_sub(&self, other: Money) -> Result<Money, MoneyError> {
        self.same(&other)?;
        let minor = self
            .minor
            .checked_sub(other.minor)
            .ok_or(MoneyError::Overflow)?;
        Ok(Money::new(minor, self.currency))
    }

    /// The amount times a whole number (three months of costs, say).
    pub fn checked_mul(&self, times: i64) -> Result<Money, MoneyError> {
        let minor = self.minor.checked_mul(times).ok_or(MoneyError::Overflow)?;
        Ok(Money::new(minor, self.currency))
    }

    /// Add up amounts in one currency; an empty list is zero.
    pub fn sum(currency: Currency, items: &[Money]) -> Result<Money, MoneyError> {
        items
            .iter()
            .try_fold(Money::zero(currency), |acc, m| acc.checked_add(*m))
    }

    pub fn is_negative(&self) -> bool {
        self.minor < 0
    }
}

impl fmt::Display for Money {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.format())
    }
}

/// An exact decimal: `mantissa / 10^scale`. Used for exchange rates; never
/// for amounts, which are [`Money`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Decimal {
    pub mantissa: i128,
    pub scale: u8,
}

/// More digits after the point than any published rate needs.
pub const MAX_SCALE: u8 = 18;

impl Decimal {
    /// Read "83.4567", "0.0118", "1". No exponents, no grouping.
    pub fn parse(text: &str) -> Result<Decimal, MoneyError> {
        let bad = || MoneyError::NotARate(text.to_owned());
        let t = text.trim();
        let (negative, t) = match t.strip_prefix('-') {
            Some(r) => (true, r),
            None => (false, t),
        };
        let (whole, frac) = t.split_once('.').unwrap_or((t, ""));
        let ok = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
        if whole.is_empty() || !ok(whole) || !ok(frac) || frac.len() > MAX_SCALE as usize {
            return Err(bad());
        }
        let mantissa: i128 = format!("{whole}{frac}").parse().map_err(|_| bad())?;
        Ok(Decimal {
            mantissa: if negative { -mantissa } else { mantissa },
            scale: frac.len() as u8,
        })
    }

    pub fn is_positive(&self) -> bool {
        self.mantissa > 0
    }
}

impl fmt::Display for Decimal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sign = if self.mantissa < 0 { "-" } else { "" };
        let digits = self.mantissa.unsigned_abs().to_string();
        let scale = self.scale as usize;
        if scale == 0 {
            return write!(f, "{sign}{digits}");
        }
        let padded = format!("{digits:0>width$}", width = scale + 1);
        let (w, fr) = padded.split_at(padded.len() - scale);
        write!(f, "{sign}{w}.{fr}")
    }
}

impl Serialize for Decimal {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Decimal {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Decimal, D::Error> {
        let s = String::deserialize(d)?;
        Decimal::parse(&s).map_err(serde::de::Error::custom)
    }
}

/// How many units of `to` one unit of `from` buys, exactly as published
/// (1 GBP = 112.3456 INR).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rate {
    pub from: Currency,
    pub to: Currency,
    pub value: Decimal,
}

impl Rate {
    pub fn new(from: Currency, to: Currency, value: &str) -> Result<Rate, MoneyError> {
        let value = Decimal::parse(value)?;
        if !value.is_positive() {
            return Err(MoneyError::NotARate(value.to_string()));
        }
        Ok(Rate { from, to, value })
    }

    /// Convert `amount` (in `from`) to `to`, rounding once, half to even, to
    /// the minor unit of `to`. All the sums are whole numbers in i128.
    pub fn convert(&self, amount: Money) -> Result<Money, MoneyError> {
        if amount.currency != self.from {
            return Err(MoneyError::WrongRate {
                want: self.from,
                got: amount.currency,
            });
        }
        // minor_to = minor_from * rate * 10^exp_to / 10^exp_from
        //          = minor_from * mantissa * 10^exp_to / (10^scale * 10^exp_from)
        let pow = |e: u32| 10i128.checked_pow(e).ok_or(MoneyError::Overflow);
        let num = (amount.minor as i128)
            .checked_mul(self.value.mantissa)
            .and_then(|n| n.checked_mul(pow(self.to.exponent() as u32).ok()?))
            .ok_or(MoneyError::Overflow)?;
        let den = pow(self.value.scale as u32)?
            .checked_mul(pow(self.from.exponent() as u32)?)
            .ok_or(MoneyError::Overflow)?;
        let minor = i64::try_from(div_half_even(num, den)).map_err(|_| MoneyError::Overflow)?;
        Ok(Money::new(minor, self.to))
    }
}

/// `num / den` rounded to the nearest whole number, ties to the even one
/// (banker's rounding), so many conversions do not drift one way. `den > 0`.
pub fn div_half_even(num: i128, den: i128) -> i128 {
    debug_assert!(den > 0);
    let q = num / den;
    let r = (num % den).abs();
    let away = if num < 0 { -1 } else { 1 };
    match (2 * r).cmp(&den) {
        std::cmp::Ordering::Greater => q + away,
        std::cmp::Ordering::Equal if q % 2 != 0 => q + away,
        _ => q,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cur(c: &str) -> Currency {
        Currency::new(c).unwrap()
    }

    #[test]
    fn the_table_is_sorted_and_knows_the_odd_exponents() {
        assert!(CURRENCIES.windows(2).all(|w| w[0].0 < w[1].0));
        assert!(CURRENCIES.len() > 150);
        for (code, exp) in [
            ("GBP", 2),
            ("INR", 2),
            ("AED", 2),
            ("USD", 2),
            ("EUR", 2),
            ("JPY", 0),
            ("KRW", 0),
            ("KWD", 3),
            ("BHD", 3),
            ("OMR", 3),
            ("UYW", 4),
        ] {
            assert_eq!(cur(code).exponent(), exp, "{code}");
        }
        assert_eq!(cur("gbp"), Currency::GBP);
        assert!(Currency::new("XYZ").is_err());
        assert!(Currency::new("").is_err());
        assert_eq!(Currency::all().count(), CURRENCIES.len());
    }

    #[test]
    fn parses_amounts_as_typed_and_refuses_extra_digits() {
        let gbp = Currency::GBP;
        assert_eq!(Money::parse("1,234.56", gbp).unwrap().minor, 123456);
        assert_eq!(Money::parse("1234.5", gbp).unwrap().minor, 123450);
        assert_eq!(Money::parse(" -12 ", gbp).unwrap().minor, -1200);
        assert_eq!(Money::parse("+3", gbp).unwrap().minor, 300);
        assert_eq!(Money::parse(".5", gbp).unwrap().minor, 50);
        assert_eq!(Money::parse("1 000", gbp).unwrap().minor, 100000);
        assert_eq!(
            Money::parse("1,23,456.78", Currency::INR).unwrap().minor,
            12345678
        );
        assert_eq!(Money::parse("1500", cur("JPY")).unwrap().minor, 1500);
        assert_eq!(Money::parse("1.234", cur("KWD")).unwrap().minor, 1234);
        assert!(matches!(
            Money::parse("1.234", gbp),
            Err(MoneyError::TooPrecise { exponent: 2, .. })
        ));
        assert!(Money::parse("1.5", cur("JPY")).is_err());
        for bad in ["", "-", "abc", "1.2.3", "1e5", "£5", "."] {
            assert!(Money::parse(bad, gbp).is_err(), "{bad:?}");
        }
        assert_eq!(
            Money::parse("99999999999999999999", gbp),
            Err(MoneyError::Overflow)
        );
    }

    #[test]
    fn formats_plainly_and_for_people() {
        let m = |minor, c| Money::new(minor, cur(c));
        assert_eq!(m(123456, "GBP").decimal(), "1234.56");
        assert_eq!(m(-5, "GBP").decimal(), "-0.05");
        assert_eq!(m(1500, "JPY").decimal(), "1500");
        assert_eq!(m(1234, "KWD").decimal(), "1.234");
        assert_eq!(m(123456789, "GBP").format(), "1,234,567.89 GBP");
        assert_eq!(m(-100000, "INR").format(), "-1,000.00 INR");
        assert_eq!(m(999, "JPY").to_string(), "999 JPY");
        // Every currency round-trips through its own decimal form.
        for c in Currency::all() {
            let x = Money::new(-1234567, c);
            assert_eq!(Money::parse(&x.decimal(), c).unwrap(), x, "{c}");
        }
    }

    #[test]
    fn arithmetic_is_checked_and_never_mixes_currencies() {
        let a = Money::new(1050, Currency::GBP);
        assert_eq!(a.checked_add(a).unwrap().minor, 2100);
        assert_eq!(
            a.checked_sub(Money::new(2000, Currency::GBP))
                .unwrap()
                .minor,
            -950
        );
        assert_eq!(a.checked_mul(3).unwrap().minor, 3150);
        assert_eq!(
            a.checked_add(Money::new(1, Currency::INR)),
            Err(MoneyError::Mismatch(Currency::GBP, Currency::INR))
        );
        let max = Money::new(i64::MAX, Currency::GBP);
        assert_eq!(max.checked_add(a), Err(MoneyError::Overflow));
        assert_eq!(max.checked_mul(2), Err(MoneyError::Overflow));
        assert_eq!(Money::sum(Currency::GBP, &[a, a, a]).unwrap().minor, 3150);
        assert_eq!(Money::sum(Currency::GBP, &[]).unwrap().minor, 0);
        assert!(Money::sum(Currency::AED, &[a]).is_err());
    }

    #[test]
    fn money_and_rates_serialise_exactly() {
        let m = Money::new(2000000, Currency::GBP);
        let j = serde_json::to_string(&m).unwrap();
        assert_eq!(j, r#"{"minor":2000000,"currency":"GBP"}"#);
        assert_eq!(serde_json::from_str::<Money>(&j).unwrap(), m);
        assert!(serde_json::from_str::<Money>(r#"{"minor":1,"currency":"ZZZ"}"#).is_err());
        let r = Rate::new(Currency::GBP, Currency::INR, "112.3456").unwrap();
        let j = serde_json::to_string(&r).unwrap();
        assert!(j.contains(r#""value":"112.3456""#), "{j}");
        assert_eq!(serde_json::from_str::<Rate>(&j).unwrap(), r);
    }

    #[test]
    fn decimals_read_and_print_exactly() {
        for s in ["83.4567", "0.0118", "1", "0.000000000000000001", "-2.50"] {
            assert_eq!(Decimal::parse(s).unwrap().to_string(), s);
        }
        for bad in ["", ".5", "1.2.3", "1e3", "abc", "0.0000000000000000001"] {
            assert!(Decimal::parse(bad).is_err(), "{bad:?}");
        }
        assert!(Rate::new(Currency::GBP, Currency::INR, "0").is_err());
        assert!(Rate::new(Currency::GBP, Currency::INR, "-1").is_err());
    }

    #[test]
    fn half_even_rounding() {
        assert_eq!(div_half_even(5, 2), 2); // 2.5 -> 2
        assert_eq!(div_half_even(7, 2), 4); // 3.5 -> 4
        assert_eq!(div_half_even(-5, 2), -2);
        assert_eq!(div_half_even(-7, 2), -4);
        assert_eq!(div_half_even(26, 10), 3);
        assert_eq!(div_half_even(24, 10), 2);
        assert_eq!(div_half_even(-26, 10), -3);
        assert_eq!(div_half_even(0, 7), 0);
        assert_eq!(div_half_even(10, 5), 2);
    }

    #[test]
    fn converts_between_currencies_with_one_rounding() {
        // £100.00 at 112.3456 = ₹11,234.56 exactly.
        let r = Rate::new(Currency::GBP, Currency::INR, "112.3456").unwrap();
        assert_eq!(
            r.convert(Money::new(10000, Currency::GBP)).unwrap(),
            Money::new(1123456, Currency::INR)
        );
        // £0.01 at 112.3456 = ₹1.123456 -> 112 paise.
        assert_eq!(r.convert(Money::new(1, Currency::GBP)).unwrap().minor, 112);
        // Ties go to the even penny: 2 fils at 0.25 = 0.5p -> 0p; 6 fils = 1.5p -> 2p.
        let half = Rate::new(Currency::AED, Currency::GBP, "0.25").unwrap();
        assert_eq!(half.convert(Money::new(2, Currency::AED)).unwrap().minor, 0);
        assert_eq!(half.convert(Money::new(6, Currency::AED)).unwrap().minor, 2);
        assert_eq!(
            half.convert(Money::new(-6, Currency::AED)).unwrap().minor,
            -2
        );
        // Exponents differ: ¥1,000 at 0.0052 GBP = £5.20; 1 KWD at 2.6 GBP = £2.60;
        // £1.00 at 0.385 KWD = 0.385 KWD (385 fils).
        let jpy = Rate::new(cur("JPY"), Currency::GBP, "0.0052").unwrap();
        assert_eq!(
            jpy.convert(Money::new(1000, cur("JPY"))).unwrap().minor,
            520
        );
        let kwd = Rate::new(cur("KWD"), Currency::GBP, "2.6").unwrap();
        assert_eq!(
            kwd.convert(Money::new(1000, cur("KWD"))).unwrap().minor,
            260
        );
        let to_kwd = Rate::new(Currency::GBP, cur("KWD"), "0.385").unwrap();
        assert_eq!(
            to_kwd
                .convert(Money::new(100, Currency::GBP))
                .unwrap()
                .minor,
            385
        );
        // £1.00 at 150.5 JPY = ¥150.5 -> ¥150 (tie to even); £3.00 -> ¥451.5 -> ¥452.
        let gbp_jpy = Rate::new(Currency::GBP, cur("JPY"), "150.5").unwrap();
        assert_eq!(
            gbp_jpy
                .convert(Money::new(100, Currency::GBP))
                .unwrap()
                .minor,
            150
        );
        assert_eq!(
            gbp_jpy
                .convert(Money::new(300, Currency::GBP))
                .unwrap()
                .minor,
            452
        );
        // The wrong currency, and amounts too large, are refused.
        assert!(matches!(
            r.convert(Money::new(1, Currency::INR)),
            Err(MoneyError::WrongRate { .. })
        ));
        let huge = Rate::new(Currency::GBP, Currency::INR, "999999999.999999999").unwrap();
        assert_eq!(
            huge.convert(Money::new(i64::MAX, Currency::GBP)),
            Err(MoneyError::Overflow)
        );
    }
}
