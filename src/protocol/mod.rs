pub mod hci;
pub mod att;

use nom::le_u8;

named!(pub parse_uuid_128<&[u8], [u8; 16]>, count_fixed!(u8, le_u8, 16));

