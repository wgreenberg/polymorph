
use deku::bitvec::{BitSlice, BitVec, Msb0};
use deku::ctx::BitSize;
use deku::{DekuRead, DekuError};
use deku::prelude::*;

pub fn vlq_read(mut rest: &BitSlice<u8, Msb0>) -> Result<(&BitSlice<u8, Msb0>, u32), DekuError> {
    let mut out = 0u32;
    loop {
        let (new_rest, value) = u8::read(rest, BitSize(8))?;
        rest = new_rest;
        out |= (value as u32) & 0x7F;
        if (value & 0x80) == 0 {
            break;
        }
        out <<= 7;
    }
    Ok((rest, out))
}

pub fn vlq_write(mut value: u32, output: &mut BitVec<u8, Msb0>) -> Result<(), DekuError> {
    loop {
        let mut x = (value & 0x7F) as u8;
        value >>= 7;

        if value != 0 {
            x |= 0x80;
        }

        x.write(output, BitSize(8))?;

        if value == 0 {
            break;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::convert::TryInto;

    #[test]
    fn test_vlq() {
        #[derive(Debug, DekuRead, DekuWrite)]
        struct TestVLQ {
            #[deku(reader = "vlq_read(deku::rest)", writer = "vlq_write(*value1, deku::output)")]
            pub value1: u32,
            #[deku(reader = "vlq_read(deku::rest)", writer = "vlq_write(*value2, deku::output)")]
            pub value2: u32,
        }

        let buf = vec![0xFF, 0x7F, 0x03];
        let test = TestVLQ::from_bytes((&buf, 0)).unwrap().1;

        assert_eq!(test.value1, 0x3FFF);
        assert_eq!(test.value2, 0x03);

        let buf2: Vec<u8> = test.try_into().unwrap();
        assert_eq!(buf, buf2);
    }
}
