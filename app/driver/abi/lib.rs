#![no_std]

//! Rust wire contract for include/hwid_reader.h. Never cast untrusted reply bytes
//! to these structures. Validate lengths, version, reserved fields and tokens.

#[cfg(test)]
extern crate std;

pub const VERSION: u32 = 1;
pub const MAX_SLOTS: usize = 32;
pub const MAX_SPD_BYTES: usize = 1024;
pub const IOCTL_CAPABILITIES: u32 = 0x8000_2000;
pub const IOCTL_ENUMERATE_SPD: u32 = 0x8000_2004;
pub const IOCTL_READ_SPD: u32 = 0x8000_2008;
pub const IOCTL_RESERVED_PPIN: u32 = 0x8000_2040;
pub const IOCTL_RESERVED_DSN: u32 = 0x8000_2044;

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Header {
    pub version: u32,
    pub size: u32,
    pub reserved: [u32; 2],
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Capabilities {
    pub header: Header,
    pub status: u32,
    pub features: u32,
    pub max_slots: u32,
    pub max_spd_bytes: u32,
    pub backend: u32,
    pub reserved: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Slot {
    pub token: u64,
    pub generation: u32,
    pub controller: u32,
    pub flags: u32,
    pub reserved: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct EnumerateReply {
    pub header: Header,
    pub status: u32,
    pub count: u32,
    pub slots: [Slot; MAX_SLOTS],
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct ReadRequest {
    pub header: Header,
    pub token: u64,
}

#[repr(C)]
pub struct ReadReply {
    pub header: Header,
    pub status: u32,
    pub byte_count: u32,
    pub token: u64,
    pub generation: u32,
    pub flags: u32,
    pub bytes: [u8; MAX_SPD_BYTES],
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct ReservedReply {
    pub header: Header,
    pub status: u32,
    pub reserved: u32,
}

const _: () = {
    assert!(core::mem::size_of::<Header>() == 16);
    assert!(core::mem::size_of::<Capabilities>() == 40);
    assert!(core::mem::size_of::<Slot>() == 24);
    assert!(core::mem::size_of::<EnumerateReply>() == 792);
    assert!(core::mem::size_of::<ReadRequest>() == 24);
    assert!(core::mem::size_of::<ReadReply>() == 1064);
    assert!(core::mem::offset_of!(ReadReply, bytes) == 40);
    assert!(core::mem::size_of::<ReservedReply>() == 24);
};

pub fn request_header(size: u32) -> [u8; 16] {
    let mut bytes = [0; 16];
    bytes[..4].copy_from_slice(&VERSION.to_le_bytes());
    bytes[4..8].copy_from_slice(&size.to_le_bytes());
    bytes
}

pub fn read_request(token: u64) -> [u8; 24] {
    let mut bytes = [0; 24];
    bytes[..16].copy_from_slice(&request_header(24));
    bytes[16..].copy_from_slice(&token.to_le_bytes());
    bytes
}

fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(
        bytes[offset..offset + 4]
            .try_into()
            .expect("validated length"),
    )
}

fn u64_at(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(
        bytes[offset..offset + 8]
            .try_into()
            .expect("validated length"),
    )
}

fn header_valid(bytes: &[u8], size: usize) -> bool {
    bytes.len() == size
        && u32_at(bytes, 0) == VERSION
        && u32_at(bytes, 4) as usize == size
        && bytes[8..16] == [0; 8]
}

/// Validate a success reply before parsing identifiers. Failure replies are not
/// successful SPD input, even if they contain nonzero data or a plausible token.
pub fn spd_reply(bytes: &[u8], token: u64, generation: u32) -> Option<&[u8]> {
    if !header_valid(bytes, 1064)
        || u32_at(bytes, 16) != 0
        || u32_at(bytes, 20) != 256
        || token == 0
        || u64_at(bytes, 24) != token
        || !matches!(generation, 3 | 4)
        || u32_at(bytes, 32) != generation
        || u32_at(bytes, 36) != u32::from(generation == 4)
        || bytes[296..].iter().any(|&b| b != 0)
    {
        return None;
    }
    Some(&bytes[40..296])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consumes_reply_from_the_actual_c_dispatcher() {
        let exe = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("out/host/host.exe");
        let output = std::process::Command::new(exe)
            .arg("--wire")
            .output()
            .expect("run check-host.ps1 to compile the SDK-only C harness first");
        assert!(output.status.success());
        let hex = std::str::from_utf8(&output.stdout).unwrap().trim();
        assert_eq!(hex.len(), 1064 * 2);
        let bytes: std::vec::Vec<u8> = (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
            .collect();
        let data = spd_reply(&bytes, 0x1_0000_0001, 3).unwrap();
        assert_eq!(data[1], 1 ^ 0x5a);
        assert_eq!(data[2], 0x0b);
    }

    #[test]
    fn buffered_ioctl_contract_and_fake_round_trip() {
        let ioctls = [
            IOCTL_CAPABILITIES,
            IOCTL_ENUMERATE_SPD,
            IOCTL_READ_SPD,
            IOCTL_RESERVED_PPIN,
            IOCTL_RESERVED_DSN,
        ];
        for ioctl in ioctls {
            assert_eq!(ioctl >> 16, 0x8000);
            assert_eq!(ioctl & 3, 0); // METHOD_BUFFERED
            assert_eq!((ioctl >> 14) & 3, 0); // FILE_ANY_ACCESS
        }
        let token = 0x1732_5000_0000_0001;
        let request = read_request(token);
        assert_eq!(u64_at(&request, 16), token);
        assert!(header_valid(&request, 24));
        let mut fake = [0u8; 1064];
        fake[..16].copy_from_slice(&request_header(1064));
        fake[20..24].copy_from_slice(&256u32.to_le_bytes());
        fake[24..32].copy_from_slice(&token.to_le_bytes());
        fake[32..36].copy_from_slice(&3u32.to_le_bytes());
        fake[40..296].fill(0x5a);
        assert_eq!(spd_reply(&fake, token, 3), Some(&fake[40..296]));
        assert!(spd_reply(&fake, token + 1, 3).is_none());
        for length in 0..1064 {
            assert!(spd_reply(&fake[..length], token, 3).is_none());
        }
        for offset in [0, 4, 8, 12, 16, 20, 24, 32, 36, 296, 1063] {
            fake[offset] ^= 1;
            assert!(spd_reply(&fake, token, 3).is_none(), "offset {offset}");
            fake[offset] ^= 1;
        }
    }
}
