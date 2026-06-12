//! Common test utilities

use airp_mcp_server::mcp::AirpMcpServer;
use airp_mcp_server::storage::Storage;
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::TempDir;

pub struct TestContext {
    pub data_dir: PathBuf,
    pub temp_dir: TempDir,
    pub storage: Arc<Storage>,
}

impl TestContext {
    pub async fn new() -> Self {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let data_dir = temp_dir.path().to_path_buf();

        let storage = Arc::new(Storage::new(&data_dir).expect("Failed to create storage"));
        storage.init().await.expect("Failed to init storage");

        Self {
            data_dir,
            temp_dir,
            storage,
        }
    }

    pub fn server(&self) -> AirpMcpServer {
        let dir_str = self.data_dir.to_string_lossy().to_string();
        AirpMcpServer::new(&dir_str).expect("Failed to create server")
    }
}

pub fn create_test_card() -> serde_json::Value {
    serde_json::json!({
        "name": "TestCharacter",
        "description": "A test character for unit testing",
        "personality": "Friendly and helpful",
        "scenario": "Test scenario",
        "first_mes": "Hello! I am a test character.",
        "mes_example": "<START>\nUser: Hello\nTestCharacter: Hi there!\n",
        "tags": ["test", "character"],
        "creator": "TestCreator",
        "character_version": "1.0"
    })
}

pub fn card_to_base64(card: &serde_json::Value) -> String {
    use base64::Engine;

    let card_json = serde_json::to_string(card).expect("Failed to serialize card");

    // Build a minimal PNG with a chara text chunk
    let png_data = build_minimal_png(&card_json);
    base64::engine::general_purpose::STANDARD.encode(&png_data)
}

fn build_minimal_png(chara_json: &str) -> Vec<u8> {
    use base64::Engine;
    use std::io::Write;

    let mut data = Vec::new();

    // PNG signature
    data.extend_from_slice(&[137, 80, 78, 71, 13, 10, 26, 10]);

    // IHDR chunk
    let ihdr_data = create_ihdr(1, 1);
    write_chunk(&mut data, b"IHDR", &ihdr_data);

    // chara chunk: SillyTavern V2 stores base64(JSON) in the chunk text, then
    // zTXt-compresses it. The importer zlib-decompresses, then base64-decodes —
    // so the chunk must carry base64(JSON), not raw JSON.
    let chara_b64 = base64::engine::general_purpose::STANDARD.encode(chara_json.as_bytes());
    let chara_data = create_ztext(b"chara", chara_b64.as_bytes());
    write_chunk(&mut data, b"zTXt", &chara_data);

    // IDAT chunk (minimal image data)
    let idat_data = create_idat();
    write_chunk(&mut data, b"IDAT", &idat_data);

    // IEND chunk
    write_chunk(&mut data, b"IEND", &[]);

    data
}

fn create_ihdr(width: u32, height: u32) -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(&width.to_be_bytes());
    data.extend_from_slice(&height.to_be_bytes());
    data.push(8); // bit depth
    data.push(2); // color type (RGB)
    data.push(0); // compression
    data.push(0); // filter
    data.push(0); // interlace
    data
}

fn create_ztext(keyword: &[u8], text: &[u8]) -> Vec<u8> {
    use std::io::Write;
    let mut data = Vec::new();
    data.extend_from_slice(keyword);
    data.push(0);
    data.push(0); // compression method

    // Compress text with zlib
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(text).unwrap();
    data.extend_from_slice(&encoder.finish().unwrap());
    data
}

fn create_idat() -> Vec<u8> {
    // Minimal RGB image data: filter byte + RGB
    let raw = vec![0, 255, 0, 0]; // filter=0, RGB=red

    use std::io::Write;
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(&raw).unwrap();
    encoder.finish().unwrap()
}

fn write_chunk(data: &mut Vec<u8>, chunk_type: &[u8; 4], chunk_data: &[u8]) {
    let len = (chunk_data.len() as u32).to_be_bytes();
    data.extend_from_slice(&len);
    data.extend_from_slice(chunk_type);
    data.extend_from_slice(chunk_data);
    let crc = compute_crc(chunk_type, chunk_data);
    data.extend_from_slice(&crc.to_be_bytes());
}

fn compute_crc(chunk_type: &[u8; 4], chunk_data: &[u8]) -> u32 {
    let mut hasher = crc32::Crc32::new();
    hasher.update(chunk_type);
    hasher.update(chunk_data);
    hasher.finalize()
}

mod crc32 {
    pub struct Crc32 {
        crc: u32,
    }

    impl Crc32 {
        pub fn new() -> Self {
            Self { crc: 0xFFFFFFFF }
        }

        pub fn update(&mut self, data: &[u8]) {
            for &byte in data {
                let index = ((self.crc as u8) ^ byte) as usize;
                self.crc = (self.crc >> 8) ^ CRC_TABLE[index];
            }
        }

        pub fn finalize(&self) -> u32 {
            self.crc ^ 0xFFFFFFFF
        }
    }

    static CRC_TABLE: [u32; 256] = {
        let mut table = [0u32; 256];
        let mut i = 0;
        while i < 256 {
            let mut crc = i as u32;
            let mut j = 0;
            while j < 8 {
                if crc & 1 != 0 {
                    crc = 0xEDB88320 ^ (crc >> 1);
                } else {
                    crc >>= 1;
                }
                j += 1;
            }
            table[i] = crc;
            i += 1;
        }
        table
    };
}
