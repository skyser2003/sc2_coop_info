pub struct ReplayFormat;

impl ReplayFormat {
    pub fn convert_fourcc(bytes: &[u8]) -> String {
        let mut s = String::new();
        for byte in bytes {
            if *byte != 0 {
                s.push(*byte as char);
            }
        }
        s
    }

    pub fn cache_handle_uri(handle: &[u8]) -> Option<String> {
        if handle.len() < 8 {
            return None;
        }

        let purpose = Self::convert_fourcc(&handle[0..4]);
        let region = Self::convert_fourcc(&handle[4..8]);
        let hash = Self::bytes_to_hex(&handle[8..]);
        if purpose.is_empty() || region.is_empty() {
            return None;
        }

        Some(format!(
            "http://{}.depot.battle.net:1119/{}.{}",
            region.to_ascii_lowercase(),
            hash.to_ascii_lowercase(),
            purpose.to_ascii_lowercase()
        ))
    }

    fn bytes_to_hex(value: &[u8]) -> String {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut out = String::with_capacity(value.len() * 2);
        for byte in value {
            out.push(HEX[(byte >> 4) as usize] as char);
            out.push(HEX[(byte & 0x0f) as usize] as char);
        }
        out
    }
}
