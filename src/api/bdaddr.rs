//! Implementation of Bluetooth's MAC address.

use std::fmt::{self, Debug, Display, Formatter, LowerHex, UpperHex};
use std::str::FromStr;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
#[cfg(feature = "serde")]
use serde_cr as serde;

/// Stores the 6 byte address used to identify Bluetooth devices.
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(crate = "serde_cr")
)]
#[derive(Copy, Clone, Hash, Eq, PartialEq, Default)]
pub struct BDAddr {
    address: [u8; 6],
}

#[derive(Debug, thiserror::Error, Clone, PartialEq)]
pub enum ParseBDAddrError {
    #[error("Bluetooth address has to be 6 bytes long")]
    IncorrectByteCount,
    #[error("Invalid digit in address: {0}")]
    InvalidDigit(#[from] std::num::ParseIntError),
}

impl Display for BDAddr {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        <Self as UpperHex>::fmt(self, f)
    }
}

impl LowerHex for BDAddr {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let a = &self.address;
        write!(
            f,
            "{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
            a[0], a[1], a[2], a[3], a[4], a[5]
        )
    }
}

impl UpperHex for BDAddr {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let a = &self.address;
        write!(
            f,
            "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
            a[0], a[1], a[2], a[3], a[4], a[5]
        )
    }
}

impl Debug for BDAddr {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        <Self as Display>::fmt(self, f)
    }
}

impl AsRef<[u8]> for BDAddr {
    fn as_ref(&self) -> &[u8] {
        &self.address
    }
}

impl From<[u8; 6]> for BDAddr {
    /// Build an address from an array.
    ///
    /// `address[0]` will be the MSB and `address[5]` the LSB.
    ///
    /// # Example
    ///
    /// ```
    /// # use btleplug::api::BDAddr;
    /// let addr: BDAddr = [0x2A, 0xCC, 0x00, 0x34, 0xFA, 0x00].into();
    /// assert_eq!("2A:CC:00:34:FA:00", addr.to_string());
    /// ```
    fn from(address: [u8; 6]) -> Self {
        Self { address }
    }
}

impl<'a> std::convert::TryFrom<&'a [u8]> for BDAddr {
    type Error = ParseBDAddrError;

    fn try_from(slice: &'a [u8]) -> Result<Self, Self::Error> {
        if slice.len() < 6 {
            Err(ParseBDAddrError::IncorrectByteCount)
        } else {
            let mut cpy = [0; 6];
            cpy.copy_from_slice(&slice[..6]);
            Ok(cpy.into())
        }
    }
}

impl From<u64> for BDAddr {
    fn from(int: u64) -> Self {
        let mut cpy = [0; 6];
        let slice = int.to_be_bytes(); // Reverse order to have MSB on index 0
        cpy.copy_from_slice(&slice[2..]);
        cpy.into()
    }
}

impl From<BDAddr> for u64 {
    fn from(addr: BDAddr) -> Self {
        let mut slice = [0; 8];
        (&mut slice[2..]).copy_from_slice(&addr.into_inner());
        u64::from_be_bytes(slice)
    }
}

impl FromStr for BDAddr {
    type Err = ParseBDAddrError;

    /// Parses a Bluetooth address of the form `aa:bb:cc:dd:ee:ff` or of form
    /// `aabbccddeeff`.
    ///
    /// All hex-digits `[0-9a-fA-F]` are allowed.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.contains(':') {
            Self::from_str_delim(s)
        } else {
            Self::from_str_no_delim(s)
        }
    }
}

impl BDAddr {
    /// Destruct the address into the underlying array.
    pub fn into_inner(self) -> [u8; 6] {
        self.address
    }

    /// Check if this address is a randomly generated.
    pub fn is_random_static(&self) -> bool {
        self.address[5] & 0b11 == 0b11
    }

    /// Parses a Bluetooth address with colons `:` as delimiters.
    ///
    /// All hex-digits `[0-9a-fA-F]` are allowed.
    pub fn from_str_delim(s: &str) -> Result<Self, ParseBDAddrError> {
        let bytes = s
            .split(':')
            .map(|part: &str| u8::from_str_radix(part, 16).map_err(ParseBDAddrError::InvalidDigit))
            .collect::<Result<Vec<u8>, _>>()?;

        if bytes.len() == 6 {
            let mut address = [0; 6];
            address.copy_from_slice(bytes.as_slice());
            Ok(BDAddr { address })
        } else {
            Err(ParseBDAddrError::IncorrectByteCount)
        }
    }

    /// Parses a Bluetooth address without delimiters.
    ///
    /// All hex-digits `[0-9a-fA-F]` are allowed.
    pub fn from_str_no_delim(s: &str) -> Result<Self, ParseBDAddrError> {
        if s.len() != 12 {
            return Err(ParseBDAddrError::IncorrectByteCount);
        }

        let mut address = [0; 6];
        let mut cur = s;
        for byte in address.iter_mut() {
            let (part, rest) = cur.split_at(2);
            *byte = u8::from_str_radix(part, 16)?;
            cur = rest;
        }
        Ok(Self { address })
    }

    /// Writes the address without delimiters.
    pub fn write_no_delim(&self, f: &mut impl fmt::Write) -> fmt::Result {
        for b in &self.address {
            write!(f, "{:02x}", b)?;
        }
        Ok(())
    }

    /// Create a `String` with the address with no delimiters.
    ///
    /// For the more common presentation with colons use the `to_string()`
    /// method.
    pub fn to_string_no_delim(&self) -> String {
        let mut s = String::with_capacity(12);
        self.write_no_delim(&mut s)
            .expect("A String-Writer never fails");
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A BDAddr with the same value as `HEX`.
    const ADDR: BDAddr = BDAddr {
        address: [0x1f, 0x2a, 0x00, 0xcc, 0x22, 0xf1],
    };
    /// A u64 with the same value as `ADDR`.
    const HEX: u64 = 0x00_00_1f_2a_00_cc_22_f1;

    #[test]
    fn parse_addr() {
        let addr = BDAddr::from([0x2a, 0x00, 0xaa, 0xbb, 0xcc, 0xdd]);

        let result: Result<BDAddr, _> = "2a:00:aa:bb:cc:dd".parse();
        assert_eq!(result, Ok(addr));
        let result: Result<BDAddr, _> = "2a00AabbCcdd".parse();
        assert_eq!(result, Ok(addr));
        let result: Result<BDAddr, _> = "2A:00:00".parse();
        assert_eq!(result, Err(ParseBDAddrError::IncorrectByteCount));
        let result: Result<BDAddr, _> = "2A:00:AA:BB:CC:ZZ".parse();
        assert!(matches!(result, Err(ParseBDAddrError::InvalidDigit(_))));
        let result: Result<BDAddr, _> = "2A00aABbcCZz".parse();
        assert!(matches!(result, Err(ParseBDAddrError::InvalidDigit(_))));
    }

    #[test]
    fn display_addr() {
        assert_eq!(format!("{}", ADDR), "1F:2A:00:CC:22:F1");
        assert_eq!(format!("{:?}", ADDR), "1F:2A:00:CC:22:F1");
        assert_eq!(format!("{:x}", ADDR), "1f:2a:00:cc:22:f1");
        assert_eq!(format!("{:X}", ADDR), "1F:2A:00:CC:22:F1");
        assert_eq!(format!("{}", ADDR.to_string_no_delim()), "1f2a00cc22f1");
    }

    #[test]
    fn u64_to_addr() {
        let hex_addr: BDAddr = HEX.into();
        assert_eq!(hex_addr, ADDR);

        let hex_back: u64 = hex_addr.into();
        assert_eq!(HEX, hex_back);
    }

    #[test]
    fn addr_to_u64() {
        let addr_as_hex: u64 = ADDR.into();
        assert_eq!(HEX, addr_as_hex);

        let addr_back: BDAddr = addr_as_hex.into();
        assert_eq!(ADDR, addr_back);
    }
}
