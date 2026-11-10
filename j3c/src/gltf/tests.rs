use super::*;

/// One triangle mesh, a PNG texture, and `nodes` as the node list.
fn gltf(nodes: &str, roots: &str) -> String {
    format!(
        r#"{{"asset":{{"version":"2.0"}},"scenes":[{{"nodes":{roots}}}],"nodes":{nodes},
            "meshes":[{{"primitives":[{{"attributes":{{"POSITION":0}},"material":0}}]}}],
            "materials":[{{"pbrMetallicRoughness":{{"baseColorTexture":{{"index":0}}}}}}],
            "textures":[{{"source":0}}],"images":[{{"uri":"data:image/png;base64,iVBORw0KGgo="}}],
            "buffers":[{{"byteLength":36,"uri":"data:application/octet-stream;base64,AAAAAAAAAAAAAAAAAACAPwAAAAAAAAAAAAAAAAAAgD8AAAAA"}}],
            "bufferViews":[{{"buffer":0,"byteLength":36}}],
            "accessors":[{{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"}}]}}"#
    )
}

fn imported(project: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(project.join("assets/imported"))
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    names
}

#[test]
fn shared_meshes_warnings_and_cleanup() {
    let project = std::env::temp_dir().join(format!("j3_gltf_test_{}", std::process::id()));
    let _ = fs::remove_dir_all(&project);
    fs::create_dir_all(&project).unwrap();
    let scene = project.join("out.j3scene");
    let source = project.join("m.gltf");

    // Three nodes share mesh 0, one of them skinned. No texture coordinates, so the
    // primitive is skipped with a warning and the rest still comes in.
    let nodes = r#"[{"mesh":0},{"mesh":0},{"mesh":0,"skin":0}]"#;
    fs::write(&source, gltf(nodes, "[0,1,2]")).unwrap();
    let warnings = import(&project, &scene, &source, false).unwrap();
    assert!(warnings.contains(&"skinning was ignored".to_string()), "{warnings:?}");
    assert!(warnings.iter().any(|w| w.contains("TEXCOORD_0")), "{warnings:?}");
    assert_eq!(warnings.len(), 2, "{warnings:?}");

    // Same file with UVs: one mesh file for three nodes.
    let text = gltf(nodes, "[0,1,2]")
        .replace(r#""POSITION":0"#, r#""POSITION":0,"TEXCOORD_0":1"#)
        .replace(r#""type":"VEC3"}]"#, r#""type":"VEC3"},{"bufferView":0,"componentType":5126,"count":3,"type":"VEC2"}]"#);
    fs::write(&source, text).unwrap();
    import(&project, &scene, &source, false).unwrap();
    assert_eq!(imported(&project).iter().filter(|n| n.starts_with("m_1_mesh")).count(), 1);
    assert_eq!(fs::read_to_string(&scene).unwrap().matches("m_1_mesh0_prim0").count(), 3);

    // A missing child fails after the image was written; nothing of it is left.
    let before = imported(&project);
    fs::write(&source, gltf(r#"[{"mesh":0,"children":[9]}]"#, "[0]")).unwrap();
    assert!(import(&project, &scene, &source, false).is_err());
    assert_eq!(imported(&project), before);

    // A node reached from two roots must fail before it can duplicate a subtree.
    fs::write(&source, gltf(r#"[{"children":[2]},{"children":[2]},{"mesh":0}]"#, "[0,1]")).unwrap();
    assert_eq!(
        import(&project, &scene, &source, false).unwrap_err(),
        "glTF: node 2 has more than one parent"
    );
    assert_eq!(imported(&project), before);

    // Percent-encoded spaces resolve beside the glTF; a missing image is a warning.
    let external = gltf(r#"[{"mesh":0}]"#, "[0]")
        .replace("data:image/png;base64,iVBORw0KGgo=", "tex%20a.png");
    fs::write(project.join("tex a.png"), b"\x89PNG\r\n\x1a\n").unwrap();
    fs::write(project.join("a..b.png"), b"\x89PNG\r\n\x1a\n").unwrap();
    assert_eq!(uri_bytes(&project, "a..b.png").unwrap(), b"\x89PNG\r\n\x1a\n");
    assert!(uri_bytes(&project, "%2e%2e/outside.png").is_err());
    fs::write(&source, &external).unwrap();
    import(&project, &scene, &source, false).unwrap();
    assert!(imported(&project).iter().any(|name| name.ends_with("_image0.png")));
    fs::remove_file(project.join("tex a.png")).unwrap();
    fs::write(&source, external).unwrap();
    let warnings = import(&project, &scene, &source, false).unwrap();
    assert!(warnings.contains(&"some textures could not be read".to_string()));

    let pbr = gltf(r#"[{"mesh":0}]"#, "[0]").replace(r#""pbrMetallicRoughness""#, r#""normalTexture":{"index":0},"pbrMetallicRoughness""#);
    fs::write(&source, pbr).unwrap();
    let warnings = import(&project, &scene, &source, false).unwrap();
    assert!(warnings.contains(&"normal, metal/roughness, and occlusion maps were ignored".to_string()), "{warnings:?}");

    let compressed = gltf(r#"[{"mesh":0}]"#, "[0]")
        .replace("\"scenes\":", "\"extensionsRequired\":[\"KHR_draco_mesh_compression\"],\"scenes\":");
    fs::write(&source, compressed).unwrap();
    assert_eq!(
        import(&project, &scene, &source, false).unwrap_err(),
        "glTF: needs KHR_draco_mesh_compression; re-export without compression"
    );

    let _ = fs::remove_dir_all(&project);
}

#[test]
fn bake_samples_texture_at_centroid() {
    let project = std::env::temp_dir().join(format!("j3_gltf_bake_{}", std::process::id()));
    let _ = fs::remove_dir_all(&project);
    fs::create_dir_all(&project).unwrap();
    let scene = project.join("out.j3scene");
    let source = project.join("m.gltf");
    fs::write(project.join("pal.png"), include_bytes!("../../tests/programs/png_check/pal.png")).unwrap();
    // The position floats read as VEC2 UVs are (0,0), (0,1), (0,0): centroid (0, 1/3),
    // which is pixel (0, 5) of the 19x17 image.
    let text = gltf(r#"[{"mesh":0}]"#, "[0]")
        .replace("data:image/png;base64,iVBORw0KGgo=", "pal.png")
        .replace(r#""POSITION":0"#, r#""POSITION":0,"TEXCOORD_0":1"#)
        .replace(r#""type":"VEC3"}]"#, r#""type":"VEC3"},{"bufferView":0,"componentType":5126,"count":3,"type":"VEC2"}]"#);
    fs::write(&source, text).unwrap();
    let warnings = import(&project, &scene, &source, true).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    let mesh = fs::read_to_string(project.join("assets/imported/m_mesh0_prim0.j3mesh")).unwrap();
    assert!(mesh.starts_with("# j3mesh 1\n"), "{mesh}");
    let row: Vec<f32> = mesh.lines().nth(2).unwrap().split(' ').map(|v| v.parse().unwrap()).collect();
    let rgba = include_bytes!("../../tests/programs/png_check/pal.rgba");
    let at = (5 * 19) * 4;
    for c in 0..3 {
        assert!((row[6 + c] - rgba[at + c] as f32 / 255.0).abs() < 1e-3, "{row:?}");
    }
    let fragment = fs::read_to_string(&scene).unwrap();
    assert!(!fragment.contains("_image0"), "{fragment}");
    let _ = fs::remove_dir_all(&project);
}
