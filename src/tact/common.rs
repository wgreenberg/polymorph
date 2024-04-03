use std::str::FromStr;

use deku::DekuRead;

macro_rules! impl_key {
    ($name:ident) => {
        #[derive(DekuRead, Debug, PartialEq, Eq, Hash, Clone)]
        pub struct $name(pub [u8; 16]);

        impl $name {
            pub fn to_string(&self) -> String {
                let mut result = String::new();
                let $name(hex) = self;
                for b in hex {
                    result.push_str(&format!("{:x}", b));
                }
                result
            }
        }

        impl FromStr for $name {
            type Err = String;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                if s.len() != 32 {
                    return Err(format!("requires string length of 16, got {}", s.len()));
                }
                let mut key = [0; 16];
                for i in 0..16 {
                    let hex = &s[i*2..i*2+2];
                    key[i] = u8::from_str_radix(hex, 16).unwrap();
                }
                Ok(Self(key))
            }
        }
    }
}

impl_key!(CKey);
impl_key!(EKey);

pub const NULL_EKEY: EKey = EKey([0; 16]);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ekey_conversion() {
        let s = "0017a402f556fbece46c38dc431a2c9b";
        let key: EKey = EKey([0x00, 0x17, 0xa4, 0x02, 0xf5, 0x56, 0xfb, 0xec, 0xe4, 0x6c, 0x38, 0xdc, 0x43, 0x1a, 0x2c, 0x9b]);
        assert_eq!(EKey::from_str(s), Ok(key.clone()));
        assert_eq!(key.to_string(), s.to_string());
    }
}
