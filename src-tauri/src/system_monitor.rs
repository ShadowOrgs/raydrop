use serde::Serialize;
use std::sync::{Mutex, OnceLock};
use sysinfo::{Components, Disks, Networks, System};

#[derive(Debug, Clone, Serialize)]
pub struct DiskTelemetry {
    pub name: String,
    pub mount_point: String,
    pub total_bytes: u64,
    pub available_bytes: u64,
    pub used_bytes: u64,
    pub usage_percent: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct SystemTelemetry {
    pub cpu_usage_percent: f32,
    pub cpu_brand: String,
    pub cpu_cores_physical: usize,
    pub cpu_cores_logical: usize,
    pub cpu_freq_mhz: u64,
    pub ram_used_bytes: u64,
    pub ram_total_bytes: u64,
    pub ram_usage_percent: f32,
    pub swap_used_bytes: u64,
    pub swap_total_bytes: u64,
    pub gpu_name: String,
    pub gpu_usage_percent: f32,
    pub net_rx_bytes_per_sec: u64,
    pub net_tx_bytes_per_sec: u64,
    pub hostname: String,
    pub os_name: String,
    pub os_version: String,
    pub kernel_version: String,
    pub arch: String,
    pub uptime_secs: u64,
    pub cpu_temp_celsius: Option<f32>,
    pub disks: Vec<DiskTelemetry>,
}

struct MonitorState {
    sys: System,
    networks: Networks,
    disks: Disks,
    components: Components,
    last_net_sample: std::time::Instant,
    gpu_name: String,
    #[cfg(target_os = "windows")]
    pdh_gpu: Option<win_gpu::PdhGpuSampler>,
}

static MONITOR: OnceLock<Mutex<MonitorState>> = OnceLock::new();

#[cfg(target_os = "windows")]
mod win_gpu {
    use std::ffi::c_void;
    use winreg::enums::*;
    use winreg::RegKey;

    pub fn read_gpu_name_from_registry() -> String {
        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
        let base = r"SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}";
        if let Ok(class_key) = hklm.open_subkey(base) {
            for idx in ["0000", "0001", "0002"] {
                if let Ok(sub) = class_key.open_subkey(idx) {
                    if let Ok(desc) = sub.get_value::<String, _>("DriverDesc") {
                        let trimmed = desc.trim();
                        if !trimmed.is_empty()
                            && !trimmed.to_lowercase().contains("basic display")
                        {
                            return trimmed.to_string();
                        }
                    }
                }
            }
        }
        "Integrated / Standard GPU".to_string()
    }

    #[repr(C)]
    struct PDH_FMT_COUNTERVALUE {
        c_status: u32,
        double_value: f64,
    }

    const PDH_FMT_DOUBLE: u32 = 0x00000200;

    #[link(name = "pdh")]
    extern "system" {
        fn PdhOpenQueryW(sz_data_source: *const u16, dw_user_data: usize, ph_query: *mut isize) -> u32;
        fn PdhAddEnglishCounterW(
            h_query: isize,
            sz_full_counter_path: *const u16,
            dw_user_data: usize,
            ph_counter: *mut isize,
        ) -> u32;
        fn PdhCollectQueryData(h_query: isize) -> u32;
        fn PdhGetFormattedCounterValue(
            h_counter: isize,
            dw_format: u32,
            lpdw_type: *mut u32,
            p_value: *mut PDH_FMT_COUNTERVALUE,
        ) -> u32;
        fn PdhCloseQuery(h_query: isize) -> u32;
    }

    pub struct PdhGpuSampler {
        query: isize,
        counter: isize,
    }

    unsafe impl Send for PdhGpuSampler {}

    impl PdhGpuSampler {
        pub fn new() -> Option<Self> {
            unsafe {
                let mut query: isize = 0;
                if PdhOpenQueryW(std::ptr::null(), 0, &mut query) != 0 || query == 0 {
                    return None;
                }
                let path: Vec<u16> = "\\GPU Engine(*engtype_3D)\\Utilization Percentage"
                    .encode_utf16()
                    .chain(Some(0))
                    .collect();
                let mut counter: isize = 0;
                if PdhAddEnglishCounterW(query, path.as_ptr(), 0, &mut counter) != 0 || counter == 0
                {
                    let _ = PdhCloseQuery(query);
                    return None;
                }
                let _ = PdhCollectQueryData(query);
                Some(Self { query, counter })
            }
        }

        pub fn sample(&mut self) -> f32 {
            unsafe {
                if PdhCollectQueryData(self.query) != 0 {
                    return 0.0;
                }
                let mut val = PDH_FMT_COUNTERVALUE {
                    c_status: 0,
                    double_value: 0.0,
                };
                let mut ctype: u32 = 0;
                if PdhGetFormattedCounterValue(self.counter, PDH_FMT_DOUBLE, &mut ctype, &mut val)
                    == 0
                {
                    (val.double_value as f32).clamp(0.0, 100.0)
                } else {
                    0.0
                }
            }
        }
    }

    impl Drop for PdhGpuSampler {
        fn drop(&mut self) {
            unsafe {
                if self.query != 0 {
                    let _ = PdhCloseQuery(self.query);
                }
            }
        }
    }
}

pub fn collect_system_telemetry() -> SystemTelemetry {
    let lock = MONITOR.get_or_init(|| {
        let mut sys = System::new_all();
        sys.refresh_all();
        let networks = Networks::new_with_refreshed_list();
        let disks = Disks::new_with_refreshed_list();
        let components = Components::new_with_refreshed_list();
        #[cfg(target_os = "windows")]
        let gpu_name = win_gpu::read_gpu_name_from_registry();
        #[cfg(not(target_os = "windows"))]
        let gpu_name = "System GPU".to_string();

        #[cfg(target_os = "windows")]
        let pdh_gpu = win_gpu::PdhGpuSampler::new();

        Mutex::new(MonitorState {
            sys,
            networks,
            disks,
            components,
            last_net_sample: std::time::Instant::now(),
            gpu_name,
            #[cfg(target_os = "windows")]
            pdh_gpu,
        })
    });

    let mut st = lock.lock().unwrap();
    st.sys.refresh_cpu_all();
    st.sys.refresh_memory();
    st.networks.refresh();
    st.disks.refresh();
    st.components.refresh();

    let elapsed_secs = st.last_net_sample.elapsed().as_secs_f64().max(0.25);
    st.last_net_sample = std::time::Instant::now();

    let mut rx_total: u64 = 0;
    let mut tx_total: u64 = 0;
    for (_name, data) in st.networks.iter() {
        rx_total = rx_total.saturating_add(data.received());
        tx_total = tx_total.saturating_add(data.transmitted());
    }
    let net_rx_bytes_per_sec = ((rx_total as f64) / elapsed_secs).round() as u64;
    let net_tx_bytes_per_sec = ((tx_total as f64) / elapsed_secs).round() as u64;

    let cpu_usage_percent = st.sys.global_cpu_usage();
    let cpus = st.sys.cpus();
    let cpu_brand = cpus
        .first()
        .map(|c| c.brand().trim().to_string())
        .filter(|b| !b.is_empty())
        .unwrap_or_else(|| "Unknown CPU".to_string());
    let cpu_freq_mhz = cpus.first().map(|c| c.frequency()).unwrap_or(0);
    let cpu_cores_logical = cpus.len();
    let cpu_cores_physical = st.sys.physical_core_count().unwrap_or(cpu_cores_logical);

    let ram_used_bytes = st.sys.used_memory();
    let ram_total_bytes = st.sys.total_memory().max(1);
    let ram_usage_percent = ((ram_used_bytes as f64 / ram_total_bytes as f64) * 100.0) as f32;
    let swap_used_bytes = st.sys.used_swap();
    let swap_total_bytes = st.sys.total_swap();

    #[cfg(target_os = "windows")]
    let gpu_usage_percent = st
        .pdh_gpu
        .as_mut()
        .map(|g| g.sample())
        .unwrap_or(0.0);
    #[cfg(not(target_os = "windows"))]
    let gpu_usage_percent = 0.0;

    let cpu_temp_celsius = st
        .components
        .iter()
        .find(|c| {
            let l = c.label().to_lowercase();
            l.contains("cpu") || l.contains("package") || l.contains("core")
        })
        .map(|c| c.temperature());

    let mut disks = Vec::new();
    for d in st.disks.list() {
        let total = d.total_space();
        if total == 0 {
            continue;
        }
        let avail = d.available_space();
        let used = total.saturating_sub(avail);
        let pct = ((used as f64 / total as f64) * 100.0) as f32;
        disks.push(DiskTelemetry {
            name: d.name().to_string_lossy().to_string(),
            mount_point: d.mount_point().to_string_lossy().to_string(),
            total_bytes: total,
            available_bytes: avail,
            used_bytes: used,
            usage_percent: pct,
        });
    }

    SystemTelemetry {
        cpu_usage_percent,
        cpu_brand,
        cpu_cores_physical,
        cpu_cores_logical,
        cpu_freq_mhz,
        ram_used_bytes,
        ram_total_bytes,
        ram_usage_percent,
        swap_used_bytes,
        swap_total_bytes,
        gpu_name: st.gpu_name.clone(),
        gpu_usage_percent,
        net_rx_bytes_per_sec,
        net_tx_bytes_per_sec,
        hostname: System::host_name().unwrap_or_else(|| "Raydrop-Host".to_string()),
        os_name: System::name().unwrap_or_else(|| std::env::consts::OS.to_string()),
        os_version: System::os_version().unwrap_or_default(),
        kernel_version: System::kernel_version().unwrap_or_default(),
        arch: std::env::consts::ARCH.to_string(),
        uptime_secs: System::uptime(),
        cpu_temp_celsius,
        disks,
    }
}
