//! Fork: best-effort virtualization detection for the client's `type`
//! field. Only clear signals are mapped; anything uncertain returns an
//! empty string (the server then leaves the type unset) rather than a
//! guess. Detection runs once at startup; it is pure file reads, no
//! subprocesses, no network.
//!
//! Order matters: container markers first (a container on a VM must read
//! as the container, not the host hypervisor), then hypervisor/DMI
//! signals, then a conservative physical-machine heuristic.

use std::fs;
use std::path::Path;

fn read_trim(path: &str) -> String {
    fs::read_to_string(path).unwrap_or_default().trim().to_string()
}

fn read_lower(path: &str) -> String {
    read_trim(path).to_lowercase()
}

fn cgroup_contains(needle: &str) -> bool {
    for path in ["/proc/self/cgroup", "/proc/1/cgroup"] {
        if let Ok(content) = fs::read_to_string(path) {
            if content.to_lowercase().contains(needle) {
                return true;
            }
        }
    }
    false
}

/// Detect the virtualization type, or "" when unsure.
pub fn detect_type() -> String {
    // Containers first.
    if Path::new("/.dockerenv").exists() || cgroup_contains("docker") {
        return "docker".to_string();
    }
    if Path::new("/proc/vz").exists() {
        return "openvz".to_string();
    }
    if cgroup_contains("lxc") {
        return "lxc".to_string();
    }
    // Xen exposes its own hypervisor node.
    if read_lower("/sys/hypervisor/type") == "xen" || Path::new("/proc/xen").exists() {
        return "xen".to_string();
    }

    let product = read_lower("/sys/class/dmi/id/product_name");
    let vendor = read_lower("/sys/class/dmi/id/sys_vendor");
    let combined = format!("{product} {vendor}");
    let has = |needle: &str| combined.contains(needle);

    if has("vmware") {
        return "vmware".to_string();
    }
    if has("virtualbox") {
        return "virtualbox".to_string();
    }
    if has("microsoft") && product.contains("virtual machine") {
        return "hyperv".to_string();
    }
    if has("xen") {
        return "xen".to_string();
    }
    // QEMU/KVM family: DMI vendor "QEMU"/"Bochs", product "KVM", or cloud
    // vendors known to run KVM (Nitro, GCE, Vultr, DigitalOcean, OCI).
    if has("qemu") || has("bochs") || product.contains("kvm") {
        return "kvm".to_string();
    }
    for needle in ["amazon ec2", "google", "vultr", "digitalocean", "droplet", "linode", "oraclecloud"] {
        if has(needle) {
            return "kvm".to_string();
        }
    }

    // Physical machine: no hypervisor flag in cpuinfo AND a known OEM
    // vendor/product. Anything else stays empty rather than guessed.
    let cpuinfo = fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
    let hypervisor_flag = cpuinfo.lines().any(|line| {
        line.starts_with("flags") && line.split(':').nth(1).is_some_and(|flags| flags.split_whitespace().any(|f| f == "hypervisor"))
    });
    if !hypervisor_flag {
        for needle in [
            "dell", "hewlett", "lenovo", "supermicro", "asus", "gigabyte", "msi", "inspur", "fujitsu",
            "poweredge", "proliant", "thinksystem", "thinkserver",
        ] {
            if has(needle) {
                return "独服".to_string();
            }
        }
    }

    String::new()
}
