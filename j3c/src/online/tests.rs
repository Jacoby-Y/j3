//! Online asset search/fetch tests (parsers, zip, hdr; the network test is ignored).

use super::pack::{box_scale, hdr_decode, shrink};
use super::*;

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("j3_online_{name}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn ids_and_paths_are_checked() {
    assert!(safe_id("ArmChair_01").is_ok());
    for bad in ["", "../x", "a/b", "a b", &"x".repeat(81)] { assert!(safe_id(bad).is_err(), "{bad}"); }
    assert!(safe_rel("textures/a.jpg").is_ok());
    for bad in ["../a", "a/../../b", "/etc/x", "a\\b", "C:x"] { assert!(safe_rel(bad).is_err(), "{bad}"); }
    assert_eq!(url_encode("red brick/ü"), "red%20brick%2F%C3%BC");
}

#[test]
fn polyhaven_list_and_files() {
    let list = r#"{"ArmChair_01":{"name":"Arm Chair 01","type":2,"tags":["gothic","chair"],"categories":["furniture"],
            "authors":{"Kirill Sannikov":"All"},"thumbnail_url":"https://cdn.polyhaven.com/asset_img/thumbs/ArmChair_01.png?width=256"},
            "Barrel_01":{"name":"Barrel 01","tags":[],"categories":["props"],"authors":{"A":"All","B":"Mesh"},"thumbnail_url":"http://x"}}"#;
    let hits = polyhaven_hits(list, "model", "FURNITURE").unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!((hits[0].id.as_str(), hits[0].name.as_str(), hits[0].author.as_str()), ("ArmChair_01", "Arm Chair 01", "Kirill Sannikov"));
    let all = polyhaven_hits(list, "model", "").unwrap();
    assert_eq!((all.len(), all[1].author.as_str(), all[1].thumb.as_str()), (2, "A, B", ""));

    let files = r#"{"gltf":{"1k":{"gltf":{"url":"https://dl.polyhaven.org/x/ArmChair_01_1k.gltf","include":{
            "ArmChair_01.bin":{"url":"https://dl.polyhaven.org/x/ArmChair_01.bin"},
            "textures/a_diff_1k.jpg":{"url":"https://dl.polyhaven.org/x/a_diff_1k.jpg"}}}}}}"#;
    let got = polyhaven_model_files(files).unwrap();
    assert_eq!(got[0].0, "ArmChair_01_1k.gltf");
    assert_eq!(got.len(), 3);
    assert!(polyhaven_model_files(&files.replace("textures/a", "../a")).is_err());
    assert!(polyhaven_model_files(&files.replace("https://dl.polyhaven.org/x/a_", "http://dl/a_")).is_err());
    let hdr = r#"{"hdri":{"1k":{"hdr":{"url":"https://dl.polyhaven.org/sky_1k.hdr"}}},"tonemapped":{}}"#;
    assert_eq!(polyhaven_hdr_url(hdr).unwrap(), "https://dl.polyhaven.org/sky_1k.hdr");
}

#[test]
fn ambientcg_reply() {
    let json = r#"{"foundAssets":[{"assetId":"Ground111","displayName":"Ground 111",
            "previewImage":{"256-PNG":"https://acg-media.example/Ground111.png"},
            "downloadFolders":{"default":{"downloadFiletypeCategories":{"zip":{"downloads":[
              {"attribute":"2K-JPG","downloadLink":"https://ambientcg.com/get?file=Ground111_2K-JPG.zip"},
              {"attribute":"1K-JPG","downloadLink":"https://ambientcg.com/get?file=Ground111_1K-JPG.zip"}]}}}}}]}"#;
    let hits = ambientcg_hits(json).unwrap();
    assert_eq!((hits[0].id.as_str(), hits[0].name.as_str(), hits[0].kind), ("Ground111", "Ground 111", "material"));
    assert_eq!(ambientcg_zip_url(json, "Ground111").unwrap(), "https://ambientcg.com/get?file=Ground111_1K-JPG.zip");
    assert!(ambientcg_zip_url(json, "Bricks001").is_err());
}

#[test]
fn kenney_list_and_page() {
    assert_eq!(kenney_hits("town")[0].id, "fantasy-town-kit");
    assert_eq!(kenney_hits("").len(), KENNEY.len());
    let html = r#"<img src="https://kenney.nl/media/pages/assets/nature-kit/2d81fc716e-1677698893/preview.png">
            <a href="https://kenney.nl/media/pages/assets/nature-kit/37ac38a37b-1677698939/kenney_nature-kit.zip">Download</a>
            <a href='https://kenney.nl/media/pages/assets/nature-kit/37ac38a37b-1677698939/kenney_nature-kit.zip'>again</a>"#;
    assert_eq!(kenney_zip_url(html, "nature-kit").unwrap(), "https://kenney.nl/media/pages/assets/nature-kit/37ac38a37b-1677698939/kenney_nature-kit.zip");
    assert!(kenney_zip_url(html, "castle-kit").is_err());
}

#[test]
fn credit_once_per_key() {
    let root = temp("credit");
    credit(&root, "kenney:nature-kit  Nature Kit — Kenney — CC0 — https://kenney.nl/assets/nature-kit").unwrap();
    credit(&root, "kenney:nature-kit  again").unwrap();
    credit(&root, "kenney:nature-kit-2  other").unwrap();
    let text = fs::read_to_string(root.join("assets/CREDITS.txt")).unwrap();
    assert_eq!(text.lines().count(), 2, "{text}");
    let _ = fs::remove_dir_all(&root);
}

/// A zip with one stored and one deflated entry, built by hand.
fn zip_of(entries: &[(&str, &[u8], bool)]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, data, deflate) in entries {
        let body = if *deflate {
            // Stored DEFLATE block: BFINAL=1, BTYPE=00, then LEN/NLEN.
            let mut z = vec![1u8];
            z.extend((data.len() as u16).to_le_bytes());
            z.extend((!(data.len() as u16)).to_le_bytes());
            z.extend_from_slice(data);
            z
        } else {
            data.to_vec()
        };
        let method: u16 = if *deflate { 8 } else { 0 };
        let offset = out.len() as u32;
        out.extend([0x50, 0x4b, 3, 4]);
        out.extend([0u8; 4]);
        out.extend(method.to_le_bytes());
        out.extend([0u8; 8]);
        out.extend((body.len() as u32).to_le_bytes());
        out.extend((data.len() as u32).to_le_bytes());
        out.extend((name.len() as u16).to_le_bytes());
        out.extend([0u8; 2]);
        out.extend(name.as_bytes());
        out.extend(&body);
        central.extend([0x50, 0x4b, 1, 2]);
        central.extend([0u8; 6]);
        central.extend(method.to_le_bytes());
        central.extend([0u8; 8]);
        central.extend((body.len() as u32).to_le_bytes());
        central.extend((data.len() as u32).to_le_bytes());
        central.extend((name.len() as u16).to_le_bytes());
        central.extend([0u8; 12]);
        central.extend(offset.to_le_bytes());
        central.extend(name.as_bytes());
    }
    let at = out.len() as u32;
    out.extend(&central);
    out.extend([0x50, 0x4b, 5, 6, 0, 0, 0, 0]);
    out.extend((entries.len() as u16).to_le_bytes());
    out.extend((entries.len() as u16).to_le_bytes());
    out.extend((central.len() as u32).to_le_bytes());
    out.extend(at.to_le_bytes());
    out.extend([0u8; 2]);
    out
}

#[test]
fn unzip_writes_files_and_refuses_escapes() {
    let dir = temp("unzip");
    let zip = zip_of(&[("Models/GLB format/a.glb", b"glb", false), ("License.txt", b"CC0", true), ("Models/", b"", false)]);
    assert_eq!(unzip(&zip, &dir).unwrap(), 2);
    assert_eq!(fs::read(dir.join("License.txt")).unwrap(), b"CC0");
    assert_eq!(pack_models(&dir), vec![dir.join("Models/GLB format/a.glb")]);
    assert!(unzip(&zip_of(&[("../evil.txt", b"x", false)]), &dir).is_err());
    assert!(!dir.parent().unwrap().join("evil.txt").exists());
    assert!(unzip(b"not a zip at all, just some text here", &dir).is_err());
    let _ = fs::remove_dir_all(&dir);
}

fn hdr_file(w: usize, body: &[u8]) -> Vec<u8> {
    let mut b = format!("#?RADIANCE\nFORMAT=32-bit_rle_rgbe\n\n-Y 1 +X {w}\n").into_bytes();
    b.extend_from_slice(body);
    b
}

#[test]
fn hdr_flat_and_rle() {
    // (128, 0, 64, e=129): v = m * 2^-7, so 1.0, 0, 0.5 -> Reinhard 0.5, 0, 1/3.
    let (w, h, px) = hdr_decode(&hdr_file(2, &[128, 0, 64, 129, 0, 0, 0, 0])).unwrap();
    let g = |v: f32| (v.powf(1.0 / 2.2) * 255.0).round() as u8;
    assert_eq!((w, h), (2, 1));
    assert_eq!(px, vec![g(0.5), 0, g(1.0 / 3.0), 255, 0, 0, 0, 255]);

    let pixels: Vec<[u8; 4]> = (0..10).map(|i| [if i < 6 { 200 } else { 10 * i as u8 }, 7, 7, 130]).collect();
    let flat: Vec<u8> = pixels.iter().flatten().copied().collect();
    let mut rle = vec![2, 2, 0, 10];
    rle.extend([128 + 6, 200, 4, 60, 70, 80, 90]); // R: a run of 6, then 4 literals
    for c in [7u8, 7, 130] { rle.extend([128 + 10, c]); }
    assert_eq!(hdr_decode(&hdr_file(10, &rle)).unwrap(), hdr_decode(&hdr_file(10, &flat)).unwrap());
    assert!(hdr_decode(b"P6\n1 1\n").is_err());
}

#[test]
fn scaling_down() {
    assert_eq!(shrink(512, 256, 128), (128, 64));
    assert_eq!(shrink(64, 64, 128), (64, 64));
    let px = [0, 0, 0, 255, 255, 255, 255, 255, 0, 0, 0, 255, 255, 255, 255, 255];
    assert_eq!(box_scale(&px, 2, 2, 1, 1), vec![127, 127, 127, 255]);
}

#[test]
#[ignore]
fn network() {
    let cache = temp("net_cache");
    std::env::set_var("XDG_CACHE_HOME", &cache);
    let root = temp("net_project");
    assert!(search("kenney", "town", "any").unwrap().iter().any(|h| h.id == "fantasy-town-kit"));
    let Fetched::Pack(models) = fetch(&root, "kenney", "fantasy-town-kit", None).unwrap() else { panic!("not a pack") };
    assert!(models.len() >= 50, "{} models", models.len());
    let thumbs = pack_thumbs("kenney", "fantasy-town-kit", &models);
    assert!(thumbs.iter().filter(|t| t.is_some()).count() * 2 > models.len(), "most models have a preview");
    assert!(!search("polyhaven", "chair", "model").unwrap().is_empty());
    let Fetched::Model(gltf) = fetch(&root, "polyhaven", "ArmChair_01", None).unwrap() else { panic!("not a model") };
    assert!(gltf.with_file_name("ArmChair_01.bin").is_file(), "{}", gltf.display());
    assert!(!search("ambientcg", "brick", "any").unwrap().is_empty());
    let credits = fs::read_to_string(root.join("assets/CREDITS.txt")).unwrap();
    assert!(credits.contains("polyhaven:ArmChair_01  Arm Chair 01 — Kirill Sannikov — CC0"), "{credits}");
    let _ = fs::remove_dir_all(&cache);
    let _ = fs::remove_dir_all(&root);
}
