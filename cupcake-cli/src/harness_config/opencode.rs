//! OpenCode plugin installation.

use super::*;

impl OpenCodeHarness {
    /// Download the OpenCode plugin from GitHub releases
    ///
    /// Downloads cupcake.js to .opencode/plugin/ directory
    pub async fn download_plugin(target_dir: &Path, global: bool) -> Result<()> {
        // Determine plugin directory
        let plugin_dir = if global {
            dirs::config_dir()
                .ok_or_else(|| anyhow!("Could not determine config directory"))?
                .join("opencode")
                .join("plugin")
        } else {
            target_dir.join(".opencode").join("plugin")
        };

        // Create plugin directory
        cupcake_core::fs_access::create_dir_all(&plugin_dir)
            .with_context(|| format!("Failed to create plugin directory: {plugin_dir:?}"))?;

        let plugin_path = plugin_dir.join("cupcake.js");

        // Use dedicated plugin release to allow independent plugin updates
        // The plugin is forward-compatible, so latest is always safe
        let plugin_url = format!(
            "https://github.com/{GITHUB_REPO}/releases/download/opencode-plugin-latest/opencode-plugin.js"
        );
        let checksum_url = format!(
            "https://github.com/{GITHUB_REPO}/releases/download/opencode-plugin-latest/opencode-plugin.js.sha256"
        );

        println!("   Downloading OpenCode plugin (latest release)...");

        // Download the plugin
        let plugin_content = download_file(&plugin_url).await.with_context(|| {
            format!(
                "Failed to download OpenCode plugin from {plugin_url}. \
                 This may happen if the release doesn't exist yet. \
                 Try installing from source: cd cupcake-plugins/opencode && npm ci && npm run build"
            )
        })?;

        // Download and verify checksum
        match download_file(&checksum_url).await {
            Ok(checksum_content) => {
                let checksum_str = String::from_utf8_lossy(&checksum_content);
                let expected_hash = checksum_str
                    .split_whitespace()
                    .next()
                    .ok_or_else(|| anyhow!("Invalid checksum format"))?;

                // Calculate actual hash
                let mut hasher = Sha256::new();
                hasher.update(&plugin_content);
                let actual_hash = hex::encode(hasher.finalize());

                if actual_hash != expected_hash {
                    return Err(anyhow!(
                        "Checksum verification failed!\n  Expected: {}\n  Actual: {}",
                        expected_hash,
                        actual_hash
                    ));
                }

                println!("   Checksum verified");
            }
            Err(e) => {
                eprintln!("   Warning: Could not verify checksum: {e}. Proceeding anyway.");
            }
        }

        // Write plugin to disk
        cupcake_core::fs_access::write(&plugin_path, &plugin_content)
            .with_context(|| format!("Failed to write plugin file: {plugin_path:?}"))?;

        println!("   Plugin installed to: {plugin_path:?}");

        Ok(())
    }

    /// Print manual installation instructions as fallback
    pub fn print_manual_instructions() {
        eprintln!();
        eprintln!("   To manually install the OpenCode plugin:");
        eprintln!();
        eprintln!("   1. Build the plugin:");
        eprintln!("      cd cupcake-plugins/opencode && npm ci && npm run build");
        eprintln!();
        eprintln!("   2. Copy to your project:");
        eprintln!("      mkdir -p .opencode/plugin");
        eprintln!("      cp cupcake-plugins/opencode/dist/cupcake.js .opencode/plugin/");
        eprintln!();
        eprintln!("   Or download from GitHub releases:");
        eprintln!(
            "      curl -fsSL https://github.com/{GITHUB_REPO}/releases/download/opencode-plugin-latest/opencode-plugin.js \\"
        );
        eprintln!("        -o .opencode/plugin/cupcake.js");
        eprintln!();
    }
}

/// Download a file from a URL
async fn download_file(url: &str) -> Result<Vec<u8>> {
    let client = reqwest::Client::builder()
        .user_agent(format!("cupcake/{}", get_cupcake_version()))
        .build()
        .context("Failed to create HTTP client")?;

    let response = client
        .get(url)
        .send()
        .await
        .with_context(|| format!("Failed to send request to {url}"))?;

    if !response.status().is_success() {
        return Err(anyhow!(
            "HTTP request failed with status {}: {}",
            response.status(),
            url
        ));
    }

    let bytes = response
        .bytes()
        .await
        .with_context(|| "Failed to read response body")?;

    Ok(bytes.to_vec())
}
