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
                if s.len() != 16 {
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
