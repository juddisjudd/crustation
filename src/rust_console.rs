//! What a Rust server prints: the chatter every start repeats, and the link to its map's picture.

use std::collections::HashSet;

const MAP_IMAGES: &str = "https://files.facepunch.com/rust/map-images/";

/// Only ever a link on Facepunch's own host, since a line can open with a name a player chose.
pub fn map_image(line: &str) -> Option<&str> {
    let said = line.strip_prefix("[Rust.MapCache-Images]")?;
    let url = said[said.find(MAP_IMAGES)?..].split_whitespace().next()?;
    (url.len() > MAP_IMAGES.len()).then_some(url)
}

/// How a line opens and what else it holds, for what Unity, Steam and Rust say on every start.
const CHATTER: &[(&str, &str)] = &[
    ("[UnityMemory]", ""),
    ("[Physics::Module]", ""),
    ("[Subsystems]", ""),
    ("[CrashSessionMarker]", ""),
    ("[S_API] SteamAPI_Init(): Loaded local", ""),
    (
        "[S_API FAIL] Tried to access Steam interface",
        "before SteamAPI_Init succeeded",
    ),
    ("Processor: ", ""),
    ("Available Memory: ", ""),
    ("Linux Kernel and distribution: ", ""),
    ("System Language: ", ""),
    ("Keyboard Layout: ", ""),
    ("Mono path[", ""),
    ("Mono config path = ", ""),
    ("Stack overflow in unmanaged: ", ""),
    ("Shutdown handler: initialize.", ""),
    ("Fallback handler could not load library ", ""),
    ("Failed to open plugin: ", "libdiscord_partner_sdk.so"),
    ("Input System module state changed to: ", ""),
    ("PlayerPrefs - Creating folder: ", ""),
    ("Unable to load player prefs", ""),
    ("Initialize engine version: ", ""),
    ("Forcing GfxDevice: ", ""),
    ("GfxDevice: creating device client", ""),
    ("NullGfxDevice:", ""),
    ("Begin MonoManager ReloadAssembly", ""),
    ("- Loaded All Assemblies, in ", ""),
    ("- Finished resetting the current domain, in ", ""),
    ("Profiler Disabled!", ""),
    ("UnloadTime: ", ""),
    ("Unloading ", "nused "),
    ("Total: ", "FindLiveObjects"),
    ("Loading ", ".bundle"),
    ("Manifest Metadata Loaded", ""),
    ("Setup unity update hooks", ""),
    ("Setting breakpad minidump AppID", ""),
    ("IPC function call IClientUtils::", ""),
    ("Curl error ", "unity3d.com"),
    ("Shader '", "' not found"),
    ("WARNING: Shader ", ""),
    ("ERROR: Shader ", ""),
    ("The shader ", " is not supported on this platform!"),
    (
        "The image effect ",
        " not supported on the current platform",
    ),
    ("Missing shader in ", ""),
    ("3D Noise requires higher shader capabilities", ""),
    ("HDR Render Texture not supported", ""),
    ("AsyncResourceUpload failed.", ""),
    ("The referenced script ", " is missing!"),
    ("Missing ", " - creating"),
    ("Nexus endpoint and/or secret key is not set", ""),
    ("Command 'batchmode' not found", ""),
    ("Command 'nographics' not found", ""),
    ("Command 'rcon.password' not found", ""),
    (
        "\"",
        "can only be called on an active agent that has been placed on a NavMesh",
    ),
    (
        "Failed to create agent because it is not close enough to the NavMesh",
        "",
    ),
];

/// The hardware summary's headings; the indented lines under each one follow it out.
const HEADINGS: [&str; 5] = ["System", "CPU", "GPU", "Process", "Mono"];

/// What keeps a navmesh line in view.
const TROUBLE: [&str; 6] = [
    "fail",
    "error",
    "exception",
    "unable",
    "could not",
    "corrupt",
];

/// Sorts one server's output into what an operator reads and what every start repeats.
#[derive(Default)]
pub struct Noise {
    last: bool,
    booted: bool,
    echoed: HashSet<String>,
}

impl Noise {
    /// Whether this line is chatter, given the lines before it.
    pub fn hears(&mut self, line: &str) -> bool {
        let line = line.trim_end();
        // An indented or empty line belongs to the one above it.
        if !line.is_empty() && !line.starts_with(char::is_whitespace) {
            self.last = self.judge(line);
        }
        self.last
    }

    fn judge(&mut self, line: &str) -> bool {
        // Longer than a Steam name, so nobody can wear it; the id in it is the server's own.
        if line.starts_with("SteamInternal_SetMinidumpSteamID:") {
            return true;
        }
        if names_a_player(line) {
            return false;
        }
        if line.starts_with("Server startup complete") {
            self.booted = true;
            self.echoed = HashSet::new();
            return false;
        }
        if let Some(said) = line.strip_prefix("[RustNav]") {
            let said = said.to_ascii_lowercase();
            return !TROUBLE.iter().any(|word| said.contains(word));
        }
        let chatter = CHATTER
            .iter()
            .any(|(opens, holds)| line.starts_with(opens) && line.contains(holds));
        if chatter || HEADINGS.contains(&line) {
            return true;
        }
        // Rust reads its command line three times while starting and echoes it each time.
        !self.booted && is_echo(line) && !self.echoed.insert(line.to_string())
    }
}

/// `server.seed: "1"`, or the notice a replicated convar gets in its place.
fn is_echo(line: &str) -> bool {
    let named = |name: &str| {
        name.contains('.')
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_')
    };
    let convar = line
        .split_once(": \"")
        .is_some_and(|(name, value)| named(name) && value.ends_with('"'));
    convar
        || (line.starts_with("ConVar '")
            && line.ends_with(" will be replicated to all other players on the server"))
}

/// A line holding a SteamID64 is about somebody, however the name they chose makes it open.
fn names_a_player(line: &str) -> bool {
    line.match_indices("7656")
        .any(|(at, _)| line[at..].bytes().take_while(u8::is_ascii_digit).count() == 17)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOOT: &str = r#"[UnityMemory] Configuration Parameters - Can be set up in boot.config
    "memorysetup-allocator-temp-initial-block-size-main=262144"
    "memorysetup-temp-allocator-size-gfx=262144"
Processor: 12th Gen Intel(R) Core(TM) i7-12700K, 20 core(s) @ 1533 MHz
Available Memory: 64056 MB
Linux Kernel and distribution: Linux 6.18 Ubuntu 24.04 64bit
System Language: en
Keyboard Layout: default
Mono path[0] = '/servers/one/RustDedicated_Data/Managed'
Mono config path = '/servers/one/RustDedicated_Data/MonoBleedingEdge/etc'
Stack overflow in unmanaged: IP: 0x155199b859b6, fault addr: 0x563adad9cdfb
Shutdown handler: initialize.
Failed to open plugin: /servers/one/RustDedicated_Data/Plugins/libdiscord_partner_sdk.so
Input System module state changed to: Initialized.
[Physics::Module] Initialized fallback backend.
PlayerPrefs - Creating folder: /config/steamcmd/home/.config/unity3d/Facepunch Studios LTD
Unable to load player prefs
Initialize engine version: 6000.3.15x1-13 (a91cf34396ee)
[Subsystems] Discovering subsystems at path /servers/one/RustDedicated_Data/UnitySubsystems
Forcing GfxDevice: Null
GfxDevice: creating device client; kGfxThreadingModeNonThreaded
NullGfxDevice:
    Version:  NULL 1.0 [1.0]
    Renderer: Null Device
Begin MonoManager ReloadAssembly
- Loaded All Assemblies, in  0.289 seconds
- Finished resetting the current domain, in  0.001 seconds
Shader 'Soft Mask/TextMeshPro/Distance Field': fallback shader 'TextMeshPro/Mobile/Distance Field' not found
Profiler Disabled!
UnloadTime: 0.744850 ms
3D Noise requires higher shader capabilities (Shader Model 3.5 / OpenGL ES 3.0), which are not available on the current platform: graphicsShaderLevel (current/required) = 30 / 35
Bootstrap Startup
Curl error 7: Failed to connect to config.uca.cloud.unity3d.com port 443 after 5 ms: Error
[CrashSessionMarker]{"SessionStartUtc":"2026-10-11T21:55:22.006981Z","Release":"release#167398"}
[Bootstrap] Opening Bundles
Loading maps/maps.bundle
Loading shared/content.bundle
[Bootstrap] Opening Bundles done in 0.78s
System
    Name: bcdcff6d7017
    OS:   Linux 6.18 Ubuntu 24.04 64bit

CPU
    Model:  12th Gen Intel(R) Core(TM) i7-12700K

Mono
    Collects: 54


[Bootstrap] Loading Menu Prefabs done in 1.09s
Manifest Metadata Loaded
    42107 pooled strings
[Bootstrap] Bootstrap Tier0
Protocol: 2634.289.1
Setup unity update hooks
Command 'batchmode' not found
Command 'nographics' not found
server.port: "28015"
Command 'rcon.password' not found
ConVar 'allusers' will be replicated to all other players on the server
server.seed: "2082846974"
[Bootstrap] Bootstrap Tier0 done in 0.36s
HDR Render Texture not supported, disabling HDR on reflection probe.
[Bootstrap] Bootstrap Config
Command 'batchmode' not found
Command 'nographics' not found
server.port: "28015"
Command 'rcon.password' not found
ConVar 'allusers' will be replicated to all other players on the server
server.seed: "2082846974"
Server Config Loaded
Nexus endpoint and/or secret key is not set, not starting nexus connection
AsyncResourceUpload failed.
The shader Hidden/Time of Day/Scattering (UnityEngine.Shader) on effect Main Camera (TOD_Scattering) is not supported on this platform!
Missing shader in Main Camera (TOD_Scattering)
Generating procedural map of size 3700 with seed 2082846974
[65.7s] Terrain Erosion
Unloading 5 Unused Serialized files (Serialized files now loaded: 129)
Unloading 47299 unused Assets to reduce memory usage. Loaded Objects now: 1650244.
Total: 235.641445 ms (FindLiveObjects: 84.033055 ms CreateObjectMapping: 16.820097 ms MarkObjects: 119.798935 ms  DeleteObjects: 14.988834 ms)

Couldn't load server/crustation/proceduralmap.3700.2082846974.289.sav - file doesn't exist
Spawning 4340 entities from map
    done.
Missing EnvSync - creating
Missing ClanManager - creating
Clan manager will use backend type: local
[RustNav] No navmesh loaded from save, building navmesh now
[RustNav] Dropped 31335 of 120409 tiles for sitting more than 100m out to sea.
Initializing 70391 entity links
    done.
[S_API] SteamAPI_Init(): Loaded local 'steamclient.so' OK.
Setting breakpad minidump AppID = 252490
SteamInternal_SetMinidumpSteamID:  Caching Steam ID:  76561197960265728 [API loaded no]
[S_API FAIL] Tried to access Steam interface SteamNetworkingUtils004 before SteamAPI_Init succeeded.
IPC function call IClientUtils::GetConnectedUniverse took too long: 50 msec
SteamServer Initialized
Server startup complete
[Rust.MapCache-Images] Image uploaded to backend: https://files.facepunch.com/rust/map-images/ab3a/proceduralmap.3700.0.289_ab3a.jpg
[RustNav] Navmesh 'default' is now fully built in 89.58 seconds (120409 tiles).
Rust+ companion server connectivity test failed! Disabling Rust+ features.

Could not establish a TCP connection to 203.0.113.7:28083
Somebody with steamid 76561198000000001 joined from ip 198.51.100.24:55449 with platform windows
    NetworkId 76561198000000001 is 102665 (Somebody)
Saved 56,076 ents, cache(0.05), write(0.01), disk(0.01), navmesh(0.00).
[RustNav] Successfully saved navmesh (0 pending tiles) in 230.2 ms, full, main 1.7 ms
Curl error 7: Failed to connect to cdp.cloud.unity3d.com port 443 after 2 ms: Error"#;

    const WORTH_READING: &str = r#"Bootstrap Startup
[Bootstrap] Opening Bundles
[Bootstrap] Opening Bundles done in 0.78s
[Bootstrap] Loading Menu Prefabs done in 1.09s
[Bootstrap] Bootstrap Tier0
Protocol: 2634.289.1
server.port: "28015"
ConVar 'allusers' will be replicated to all other players on the server
server.seed: "2082846974"
[Bootstrap] Bootstrap Tier0 done in 0.36s
[Bootstrap] Bootstrap Config
Server Config Loaded
Generating procedural map of size 3700 with seed 2082846974
[65.7s] Terrain Erosion
Couldn't load server/crustation/proceduralmap.3700.2082846974.289.sav - file doesn't exist
Spawning 4340 entities from map
    done.
Clan manager will use backend type: local
Initializing 70391 entity links
    done.
SteamServer Initialized
Server startup complete
[Rust.MapCache-Images] Image uploaded to backend: https://files.facepunch.com/rust/map-images/ab3a/proceduralmap.3700.0.289_ab3a.jpg
Rust+ companion server connectivity test failed! Disabling Rust+ features.

Could not establish a TCP connection to 203.0.113.7:28083
Somebody with steamid 76561198000000001 joined from ip 198.51.100.24:55449 with platform windows
    NetworkId 76561198000000001 is 102665 (Somebody)
Saved 56,076 ents, cache(0.05), write(0.01), disk(0.01), navmesh(0.00)."#;

    #[test]
    fn a_start_is_left_with_the_lines_worth_reading() {
        let mut noise = Noise::default();
        let kept: Vec<&str> = BOOT.lines().filter(|line| !noise.hears(line)).collect();
        assert_eq!(kept, WORTH_READING.lines().collect::<Vec<_>>());
    }

    #[test]
    fn a_convar_set_again_after_the_start_is_shown_again() {
        let mut noise = Noise::default();
        assert!(!noise.hears("server.maxplayers: \"10\""));
        assert!(noise.hears("server.maxplayers: \"10\""));
        assert!(!noise.hears("Server startup complete"));
        assert!(!noise.hears("server.maxplayers: \"10\""));
        assert!(!noise.hears("server.maxplayers: \"10\""));
    }

    #[test]
    fn a_mistyped_command_still_answers() {
        let mut noise = Noise::default();
        assert!(!noise.hears("Command 'sever.save' not found"));
    }

    #[test]
    fn navmesh_trouble_stays_in_view() {
        let mut noise = Noise::default();
        assert!(noise.hears("[RustNav] Saving navmesh to path: server/crustation/a.navmesh..."));
        assert!(!noise.hears("[RustNav] Failed to load navmesh from save"));
        assert!(noise.hears(
            "\"SetDestination\" can only be called on an active agent that has been placed on a NavMesh."
        ));
    }

    #[test]
    fn a_name_cannot_hide_its_owner() {
        let mut noise = Noise::default();
        for line in [
            "[RustNav] bob with steamid 76561198000000001 joined from ip 198.51.100.24:55449",
            "UnloadTime: 1[76561198000000001] has spawned",
            "Curl error unity3d.com[76561198000000001] was killed by Somebody[76561198000000002]",
        ] {
            assert!(!noise.hears(line), "{line}");
        }
    }

    #[test]
    fn finds_the_picture_of_the_map() {
        let url =
            "https://files.facepunch.com/rust/map-images/ab3a/proceduralmap.3700.0.289_ab3a.jpg";
        assert_eq!(
            map_image(&format!(
                "[Rust.MapCache-Images] Image uploaded to backend: {url}"
            )),
            Some(url)
        );
        assert_eq!(
            map_image(&format!(
                "[Rust.MapCache-Images] Already there: {url} (cached)"
            )),
            Some(url)
        );
    }

    #[test]
    fn takes_a_picture_from_nowhere_else() {
        for line in [
            "[Rust.MapCache-Images] Image uploaded to backend: https://example.com/map.jpg",
            "[Rust.MapCache-Images] Image uploaded to backend: https://files.facepunch.com/rust/map-images/",
            "[Rust.MapCache] Map uploaded to backend: https://files.facepunch.com/rust/maps/cc11/a.map",
            "bob: [Rust.MapCache-Images] https://files.facepunch.com/rust/map-images/ab3a/a.jpg",
        ] {
            assert_eq!(map_image(line), None, "{line}");
        }
    }
}
