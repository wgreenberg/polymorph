pub type EKey = [u8; 16];
pub type CKey = [u8; 16];

pub fn hexstring(hex: &[u8]) -> String {
    let mut result = String::new();
    for b in hex {
        result.push_str(&format!("{:x}", b));
    }
    result
}

pub fn hexunstring(s: &str) -> [u8; 16] {
    let mut key = [0; 16];
    for i in 0..16 {
        let hex = &s[i*2..i*2+2];
        key[i] = u8::from_str_radix(hex, 16).unwrap();
    }
    key
}
