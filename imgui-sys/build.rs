#![allow(dead_code)]

// NOTE (arcdps fork): these must produce byte-identical struct layouts to the Dear ImGui
// build inside arcdps, because an arcdps addon drives arcdps's own ImGuiContext. arcdps
// builds stock imgui 1.92.7 with an unmodified imconfig.h -- same as the vcpkg `imgui`
// port that the working C++ addons (Boon Table, killproof.me) link against. So both
// IMGUI_DISABLE_OBSOLETE_FUNCTIONS (drops real struct members, e.g. ImGuiIO::FontGlobalScale)
// and IMGUI_USE_WCHAR32 (widens ImWchar) have to stay off here and in the cimgui generator.
const DEFINES: &[(&str, Option<&str>)] = &[
    // Disabled due to linking issues
    ("CIMGUI_NO_EXPORT", None),
    ("IMGUI_DISABLE_WIN32_FUNCTIONS", None),
    ("IMGUI_DISABLE_OSX_FUNCTIONS", None),
];

#[cfg(feature = "freetype")]
fn find_freetype() -> Vec<impl AsRef<std::path::Path>> {
    #[cfg(not(feature = "use-vcpkg"))]
    match pkg_config::Config::new().find("freetype2") {
        Ok(freetype) => freetype.include_paths,
        Err(err) => panic!("cannot find freetype: {}", err),
    }
    #[cfg(feature = "use-vcpkg")]
    match vcpkg::find_package("freetype") {
        Ok(freetype) => freetype.include_paths,
        Err(err) => panic!("cannot find freetype: {}", err),
    }
}

/// Parses `IMGUI_VERSION` / `IMGUI_VERSION_NUM` out of the vendored `imgui.h` and writes them
/// into `$OUT_DIR/imgui_version.rs` for `lib.rs` to include.
fn emit_imgui_version(imgui_h: &std::path::Path) -> std::io::Result<()> {
    let header = std::fs::read_to_string(imgui_h)?;

    let define = |name: &str| -> Option<String> {
        header.lines().find_map(|line| {
            let rest = line.strip_prefix("#define ")?;
            let rest = rest.strip_prefix(name)?;
            if !rest.starts_with(char::is_whitespace) {
                return None;
            }
            Some(rest.trim().trim_matches('"').to_string())
        })
    };

    let version = define("IMGUI_VERSION")
        .unwrap_or_else(|| panic!("no IMGUI_VERSION in {}", imgui_h.display()));
    let version_num = define("IMGUI_VERSION_NUM")
        .unwrap_or_else(|| panic!("no IMGUI_VERSION_NUM in {}", imgui_h.display()));

    let out = std::path::Path::new(&std::env::var_os("OUT_DIR").unwrap()).join("imgui_version.rs");
    std::fs::write(
        out,
        format!(
            "/// Dear ImGui version string of the vendored headers, e.g. `\"1.92.7\"`.\n\
             pub const IMGUI_VERSION: &str = \"{version}\";\n\
             /// Dear ImGui `IMGUI_VERSION_NUM` of the vendored headers, e.g. `19270`.\n\
             pub const IMGUI_VERSION_NUM: u32 = {version_num};\n"
        ),
    )?;
    println!("cargo:rerun-if-changed={}", imgui_h.display());
    println!("cargo:IMGUI_VERSION_NUM={version_num}");
    Ok(())
}

// Output define args for compiler
fn main() -> std::io::Result<()> {
    let external_enabled = std::env::var_os("CARGO_FEATURE_EXTERNAL").is_some();

    // Root of imgui-sys
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));

    // Feature flags - no extra dependencies, so these are queried as
    // env-vars to avoid recompilation of build.rs
    let docking_enabled = std::env::var_os("CARGO_FEATURE_DOCKING").is_some();
    let freetype_enabled = std::env::var_os("CARGO_FEATURE_FREETYPE").is_some();
    let wasm_enabled = std::env::var_os("CARGO_FEATURE_WASM").is_some();

    let cimgui_dir = manifest_dir.join(match (docking_enabled, freetype_enabled) {
        (false, false) => "third-party/imgui-master",
        (true, false) => "third-party/imgui-docking",
        (false, true) => "third-party/imgui-master-freetype",
        (true, true) => "third-party/imgui-docking-freetype",
    });

    // Lift IMGUI_VERSION/IMGUI_VERSION_NUM out of the vendored headers so downstream crates can
    // report the version they were actually compiled against, rather than a hardcoded guess.
    // arcdps refuses an addon's UI callbacks unless this number matches its own exactly.
    emit_imgui_version(&cimgui_dir.join("imgui/imgui.h"))?;

    if external_enabled {
        println!(
            "cargo:warning=feature 'external' enabled, skipping C++ compilation of libcimgui.a"
        );
        return Ok(());
    }

    println!(
        "cargo:THIRD_PARTY={}",
        manifest_dir.join("third-party").display()
    );
    for (key, value) in DEFINES.iter() {
        println!("cargo:DEFINE_{}={}", key, value.unwrap_or(""));
    }

    // For projects like implot-rs we expose the path to our cimgui
    // files, via `DEP_IMGUI_THIRD_PARTY` env-var, so they can build
    // against the same thing
    println!("cargo:THIRD_PARTY={}", cimgui_dir.display());

    // If we aren't building WASM output, bunch of extra stuff to do
    if !wasm_enabled {
        // C++ compiler
        let mut build = cc::Build::new();
        build.cpp(true);

        // imgui uses C++11 stuff from v1.87 onwards
        build.flag_if_supported("-std=c++11");

        // Set defines for compiler
        for (key, value) in DEFINES.iter() {
            build.define(key, *value);
        }

        // Freetype font rasterizer feature
        #[cfg(feature = "freetype")]
        {
            // Supress warnings:
            // warning: ‘ImFontBuildSrcGlyphFT’ has a field ‘ImFontBuildSrcGlyphFT::Info’ whose type uses the anonymous namespace
            // warning: ‘ImFontBuildSrcDataFT’ has a field ‘ImFontBuildSrcDataFT::Font’ whose type uses the anonymous namespace
            build.flag_if_supported("-Wno-subobject-linkage");

            // Include freetype headers
            for include in find_freetype() {
                build.include(include);
            }

            // Set flag for dear imgui
            build.define("CIMGUI_FREETYPE", None); // Sets IMGUI_ENABLE_FREETYPE
            println!("cargo:DEFINE_IMGUI_ENABLE_FREETYPE=");

            // imgui_freetype.cpp needs access to `#include "imgui.h"`.
            // So we include something like '[...]/third-party/imgui-master/imgui/'
            build.include(dbg!(cimgui_dir.join("imgui")));
        }

        // Which "all imgui" file to use
        let imgui_cpp = match (docking_enabled, freetype_enabled) {
            (false, false) => "include_imgui_master.cpp",
            (true, false) => "include_imgui_docking.cpp",
            (false, true) => "include_imgui_master_freetype.cpp",
            (true, true) => "include_imgui_docking_freetype.cpp",
        };

        // Set up compiler
        let compiler = build.get_compiler();

        // Avoid the if-supported flag functions for easy cases, as they're
        // kinda costly.
        if compiler.is_like_gnu() || compiler.is_like_clang() {
            build.flag("-fno-exceptions").flag("-fno-rtti");
        }

        // Build imgui lib, suppressing warnings.
        build.warnings(false).file(imgui_cpp).compile("libcimgui.a");
    }
    Ok(())
}
