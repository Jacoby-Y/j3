use super::scan::Src;
use super::{check, Finding, Opts};
use std::path::PathBuf;

const FLAT: &str = r#"
typedef unsigned int uint32;
class ISteamUserStats;
// comment
S_API ISteamUserStats * S_CALLTYPE SteamAPI_SteamUserStats_v013();
S_API ISteamUserStats * SteamAPI_SteamUserStats_v012();
S_API bool S_CALLTYPE SteamAPI_ISteamUserStats_SetAchievement( ISteamUserStats* self,
    const char * pchName );
S_API uint32 SteamAPI_ISteamUserStats_GetNumAchievements( ISteamUserStats* self );
"#;

const API: &str = r#"
#pragma once
#include <stdint.h>
#if defined(__linux__)
#define VALVE_CALLBACK_PACK_SMALL
#else
#define VALVE_CALLBACK_PACK_LARGE
#endif
enum EResult { k_EResultOK = 1 };
#if defined(VALVE_CALLBACK_PACK_SMALL)
#pragma pack(push, 4)
#else
#pragma pack(push, 8)
#endif
struct UserStatsStored_t { enum { k_iCallback = 1102 }; uint64_t m_nGameID; EResult m_eResult; };
#pragma pack(pop)
"#;

const FNS: &str = "    addAccessor(\"SteamAPI_SteamUserStats_v013\");\n    addAccessor(\"SteamAPI_SteamUserStats_v012\");\n";
const STATS: &str = "    let f = method(\"SteamUserStats\", \"SteamAPI_ISteamUserStats_SetAchievement\") as fn(ptr, ptr): bool;\n    let g = method(\"SteamUserStats\", \"SteamAPI_ISteamUserStats_GetNumAchievements\") as fn(ptr): u32;\n";
const PACK: &str = "    addLayout(\"UserStatsStored_t\", \"gameId\", 0, 0);\n    addLayout(\"UserStatsStored_t\", \"eResult\", 8, 8);\n    addLayout(\"UserStatsStored_t\", SIZE, 12, 16);\n    addCallback(\"UserStatsStored_t\", 1102);\nconst CB_STATS_STORED: i32 = 1102;\npub const NET_OK: i32 = 1;\n";

fn available() -> bool {
    super::probe::linux_cc().is_some() && super::probe::which("x86_64-w64-mingw32-g++")
}

fn sdk(name: &str, flat: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("j3-steamcheck-test-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let dir = root.join("sdk/public/steam");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("steam_api_flat.h"), flat).unwrap();
    std::fs::write(dir.join("steam_api.h"), API).unwrap();
    root.join("sdk/public")
}

fn srcs(fns: &str, stats: &str, pack: &str) -> Vec<Src> {
    let s = |f: &str, t: &str| Src { file: format!("j3/steam/{f}"), text: t.to_string() };
    vec![s("fns.j3", fns), s("stats.j3", stats), s("pack.j3", pack)]
}

fn run(name: &str, flat: &str, srcs: &[Src], compile: bool) -> Vec<Finding> {
    let public = sdk(name, flat);
    let mut out = check(&public, srcs, Opts { compile, windows: true });
    // the tiny fake stdlib lacks most named consts; the real-sources test and `missing_const_row` cover that error
    out.retain(|f| !f.text.contains("table names"));
    let _ = std::fs::remove_dir_all(public.parent().unwrap().parent().unwrap());
    out
}

fn has(f: &[Finding], error: bool, needle: &str) -> bool {
    f.iter().any(|x| x.error == error && !x.info && x.text.contains(needle))
}

fn errors(f: &[Finding]) -> Vec<String> {
    f.iter().filter(|x| x.error).map(Finding::line).collect()
}

#[test]
fn steam_check_good_sdk_has_no_errors() {
    let calls = run("good1", FLAT, &srcs(FNS, STATS, PACK), false);
    assert!(errors(&calls).is_empty(), "{:?}", errors(&calls));
    if !available() {
        eprintln!("steam_check_good: skipping the probe, clang++/g++ or x86_64-w64-mingw32-g++ is missing");
        return;
    }
    let all = run("good2", FLAT, &srcs(FNS, STATS, PACK), true);
    assert!(errors(&all).is_empty(), "{:?}", errors(&all));
    assert!(all.iter().any(|f| f.info && f.text.contains("VALVE_CALLBACK_PACK_SMALL")));
    assert!(all.iter().any(|f| f.info && f.text.contains("VALVE_CALLBACK_PACK_LARGE")));
}

#[test]
fn steam_check_renamed_symbol_is_an_error() {
    let stats = STATS.replace("SetAchievement", "SetAchievemnt");
    let f = run("rename", FLAT, &srcs(FNS, &stats, PACK), false);
    assert!(has(&f, true, "j3/steam/stats.j3:1: SteamAPI_ISteamUserStats_SetAchievemnt is not in steam_api_flat.h"), "{:?}", errors(&f));
}

#[test]
fn steam_check_arity_and_type_changes_are_errors() {
    let stats = STATS.replace("fn(ptr, ptr): bool", "fn(ptr, ptr, ptr): bool");
    let f = run("arity", FLAT, &srcs(FNS, &stats, PACK), false);
    assert!(has(&f, true, "3 params, SDK has 2"), "{:?}", errors(&f));
    let stats = STATS.replace("fn(ptr, ptr): bool", "fn(ptr, f32): bool");
    let f = run("types", FLAT, &srcs(FNS, &stats, PACK), false);
    assert!(has(&f, true, "param 2"), "{:?}", errors(&f));
    let stats = STATS.replace("): u32;", "): u64;");
    let f = run("ret", FLAT, &srcs(FNS, &stats, PACK), false);
    assert!(has(&f, true, "return"), "{:?}", errors(&f));
}

#[test]
fn steam_check_newer_accessor_is_a_warning() {
    let flat = format!("{FLAT}S_API ISteamUserStats * SteamAPI_SteamUserStats_v014();\n");
    let f = run("newer", &flat, &srcs(FNS, STATS, PACK), false);
    assert!(has(&f, false, "SteamAPI_SteamUserStats_v014"), "{:?}", f.iter().map(Finding::line).collect::<Vec<_>>());
    assert!(errors(&f).is_empty());
    let gone = FNS.replace("v013", "v099").replace("v012", "v098");
    let f = run("gone", FLAT, &srcs(&gone, STATS, PACK), false);
    assert!(has(&f, true, "none of our SteamAPI_SteamUserStats rows"), "{:?}", errors(&f));
}

#[test]
fn steam_check_wrong_offset_names_the_pack() {
    if !available() {
        eprintln!("steam_check_wrong_offset: skipping, clang++/g++ or x86_64-w64-mingw32-g++ is missing");
        return;
    }
    let pack = PACK.replace("\"eResult\", 8, 8", "\"eResult\", 8, 12");
    let f = run("offset", FLAT, &srcs(FNS, STATS, &pack), true);
    assert!(has(&f, true, "UserStatsStored_t.eResult: ours 12, SDK 8 (windows, pack 8)"), "{:?}", errors(&f));
    assert_eq!(errors(&f).len(), 1, "{:?}", errors(&f));
    let pack = PACK.replace("1102);", "1103);");
    let f = run("cbid", FLAT, &srcs(FNS, STATS, &pack), true);
    assert!(has(&f, true, "callback id ours 1103, SDK 1102"), "{:?}", errors(&f));
    let pack = PACK.replace("NET_OK: i32 = 1", "NET_OK: i32 = 2");
    let f = run("const", FLAT, &srcs(FNS, STATS, &pack), true);
    assert!(has(&f, true, "ours 2, SDK 1"), "{:?}", errors(&f));
}

#[test]
fn steam_check_reduces_c_types_to_kinds() {
    use super::header::{type_of, Types, K};
    let mut t = Types::default();
    t.add_text("typedef unsigned long long uint64;\ntypedef uint64 SteamAPICall_t;\ntypedef int int32;\ntypedef int32 HSteamPipe;\nenum EResult { k_EResultOK = 1 };\ntypedef enum { a } EFoo;\nstruct InputDigitalActionData_t { bool bState; };\ntypedef void (*Cb)(int);");
    let k = |s: &str, name: bool| t.kind(&type_of(s, name));
    assert_eq!(k("SteamAPICall_t hCall", true), K::U64);
    assert_eq!(k("HSteamPipe", false), K::I32);
    assert_eq!(k("const char * pchName", true), K::Ptr);
    assert_eq!(k("EResult eResult", true), K::I32);
    assert_eq!(k("EFoo", false), K::I32);
    assert_eq!(k("InputDigitalActionData_t", false), K::Struct);
    assert_eq!(k("Cb pfn", true), K::Ptr);
    assert_eq!(k("unsigned int", false), K::U32);
    assert_eq!(k("uint32 unAppID", true), K::U32);
    assert_eq!(k("NoSuch_t", false), K::Unknown);
}

/// The real j3.steam sources against a permissive SDK: every symbol declared with the J3 cast's own
/// arity, so only unreadable lines and unlisted consts can show up.
#[test]
fn steam_check_reads_every_line_of_the_real_sources() {
    let srcs = super::scan::sources();
    let sc = super::scan::scan(&srcs);
    assert!(sc.sites.len() > 200 && !sc.pubs.is_empty());
    let mut flat = String::new();
    for s in &sc.sites {
        let n = s.cast.as_ref().map_or(0, |c| c.params.len());
        let ps: Vec<String> = (0..n).map(|i| format!("void * p{i}")).collect();
        flat += &format!("S_API void * {}({});\n", s.sym, ps.join(", "));
    }
    let out = run("real", &flat, &srcs, false);
    let bad: Vec<String> = out.iter().filter(|f| f.text.contains("cannot read") || f.text.contains("neither CONSTS")).map(Finding::line).collect();
    assert!(bad.is_empty(), "{bad:?}");
}

#[test]
fn steam_check_table_row_for_a_missing_const_is_an_error() {
    let public = sdk("missing", FLAT);
    let out = check(&public, &srcs(FNS, STATS, PACK), Opts { compile: true, windows: false });
    let _ = std::fs::remove_dir_all(public.parent().unwrap().parent().unwrap());
    if available() {
        assert!(has(&out, true, "table names SEND_RELIABLE but j3.steam has no such const"));
    }
    let out = run("unlisted", FLAT, &srcs(FNS, STATS, &format!("{PACK}pub const BRAND_NEW: i32 = 5;\n")), false);
    assert!(has(&out, true, "pub const BRAND_NEW is in neither CONSTS nor UNCHECKED"), "{:?}", errors(&out));
}
