$ErrorActionPreference = 'Stop'

$logDir = Join-Path $env:TEMP 'Raydrop'
if (-not (Test-Path $logDir)) {
    New-Item -ItemType Directory -Path $logDir -Force | Out-Null
}
$logFile = Join-Path $logDir 'vdd-install.log'

function Write-Log {
    param([string]$Message)
    $ts = Get-Date -Format 'yyyy-MM-dd HH:mm:ss'
    Add-Content -Path $logFile -Value "[$ts] $Message" -ErrorAction SilentlyContinue
}

function Test-VddInstalled {
    try {
        $pnp = Get-PnpDevice -ErrorAction SilentlyContinue | Where-Object {
            $_.InstanceId -match 'MttVDD' -or ($_.HardwareID -and ($_.HardwareID -match 'MttVDD'))
        }
        if ($pnp) {
            return $true
        }
    } catch {}

    try {
        $rootDisplayKey = 'HKLM:\SYSTEM\CurrentControlSet\Enum\ROOT\DISPLAY'
        if (Test-Path $rootDisplayKey) {
            $subkeys = Get-ChildItem -Path $rootDisplayKey -ErrorAction SilentlyContinue
            foreach ($sk in $subkeys) {
                $props = Get-ItemProperty -Path $sk.PSPath -ErrorAction SilentlyContinue
                if ($props -and $props.HardwareID -and ($props.HardwareID -match 'MttVDD')) {
                    return $true
                }
            }
        }
    } catch {}

    return $false
}

try {
    Write-Log 'Checking Virtual Display Driver (MttVDD) installation status...'

    if (Test-VddInstalled) {
        Write-Log 'Virtual Display Driver (Root\MttVDD) is already installed. Skipping.'
        exit 0
    }

    $driverDir = $PSScriptRoot
    $cerPath = Join-Path $driverDir 'Virtual_Display_Driver.cer'
    $infPath = Join-Path $driverDir 'MttVDD.inf'
    $xmlSrc = Join-Path $driverDir 'vdd_settings.xml'

    if (-not (Test-Path $cerPath) -or -not (Test-Path $infPath)) {
        Write-Log "Driver files missing in $driverDir. Aborting."
        exit 0
    }

    # Ensure default vdd_settings.xml is available in C:\VirtualDisplayDriver if not present
    if (Test-Path $xmlSrc) {
        $vddSettingsDir = 'C:\VirtualDisplayDriver'
        $vddSettingsDest = Join-Path $vddSettingsDir 'vdd_settings.xml'
        if (-not (Test-Path $vddSettingsDest)) {
            if (-not (Test-Path $vddSettingsDir)) {
                New-Item -ItemType Directory -Path $vddSettingsDir -Force | Out-Null
            }
            Copy-Item -Path $xmlSrc -Destination $vddSettingsDest -Force -ErrorAction SilentlyContinue
            Write-Log "Copied vdd_settings.xml to $vddSettingsDest"
        }
    }

    # 1. Install certificate to LocalMachine Root and TrustedPublisher
    Write-Log "Installing certificate: $cerPath"
    $certRoot = Start-Process -FilePath 'certutil.exe' -ArgumentList @('-addstore', '-f', 'root', "`"$cerPath`"") -Wait -PassThru -WindowStyle Hidden
    $certPub = Start-Process -FilePath 'certutil.exe' -ArgumentList @('-addstore', '-f', 'TrustedPublisher', "`"$cerPath`"") -Wait -PassThru -WindowStyle Hidden
    Write-Log "certutil exit codes: root=$($certRoot.ExitCode), TrustedPublisher=$($certPub.ExitCode)"

    # 2. Create Root\MttVDD device node and install MttVDD.inf via SetupAPI + NewDev
    $csharp = @"
using System;
using System.Runtime.InteropServices;
using System.Text;

public static class VddInstaller
{
    private const uint DICD_GENERATE_ID = 0x00000001;
    private const uint SPDRP_HARDWAREID = 0x00000001;
    private const uint DIF_REGISTERDEVICE = 0x00000019;
    private const uint INSTALLFLAG_FORCE = 0x00000001;
    private static readonly IntPtr INVALID_HANDLE_VALUE = new IntPtr(-1);

    [StructLayout(LayoutKind.Sequential)]
    private struct SP_DEVINFO_DATA
    {
        public uint cbSize;
        public Guid ClassGuid;
        public uint DevInst;
        public IntPtr Reserved;
    }

    [DllImport("setupapi.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern bool SetupDiGetINFClassW(
        string InfName,
        ref Guid ClassGuid,
        StringBuilder ClassName,
        int ClassNameSize,
        out int RequiredSize);

    [DllImport("setupapi.dll", SetLastError = true)]
    private static extern IntPtr SetupDiCreateDeviceInfoList(
        ref Guid ClassGuid,
        IntPtr hwndParent);

    [DllImport("setupapi.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern bool SetupDiCreateDeviceInfoW(
        IntPtr DeviceInfoSet,
        string DeviceName,
        ref Guid ClassGuid,
        string DeviceDescription,
        IntPtr hwndParent,
        uint CreationFlags,
        ref SP_DEVINFO_DATA DeviceInfoData);

    [DllImport("setupapi.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern bool SetupDiSetDeviceRegistryPropertyW(
        IntPtr DeviceInfoSet,
        ref SP_DEVINFO_DATA DeviceInfoData,
        uint Property,
        byte[] PropertyBuffer,
        int PropertyBufferSize);

    [DllImport("setupapi.dll", SetLastError = true)]
    private static extern bool SetupDiCallClassInstaller(
        uint InstallFunction,
        IntPtr DeviceInfoSet,
        ref SP_DEVINFO_DATA DeviceInfoData);

    [DllImport("setupapi.dll", SetLastError = true)]
    private static extern bool SetupDiDestroyDeviceInfoList(IntPtr DeviceInfoSet);

    [DllImport("newdev.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern bool UpdateDriverForPlugAndPlayDevicesW(
        IntPtr hwndParent,
        string HardwareId,
        string FullInfPath,
        uint InstallFlags,
        out bool bRebootRequired);

    public static string Install(string infPath, string hardwareId)
    {
        Guid classGuid = Guid.Empty;
        StringBuilder className = new StringBuilder(32);
        int requiredSize = 0;

        if (!SetupDiGetINFClassW(infPath, ref classGuid, className, className.Capacity, out requiredSize))
        {
            return "SetupDiGetINFClassW failed: " + Marshal.GetLastWin32Error();
        }

        IntPtr deviceInfoSet = SetupDiCreateDeviceInfoList(ref classGuid, IntPtr.Zero);
        if (deviceInfoSet == INVALID_HANDLE_VALUE)
        {
            return "SetupDiCreateDeviceInfoList failed: " + Marshal.GetLastWin32Error();
        }

        try
        {
            SP_DEVINFO_DATA devInfoData = new SP_DEVINFO_DATA();
            devInfoData.cbSize = (uint)Marshal.SizeOf(typeof(SP_DEVINFO_DATA));

            if (!SetupDiCreateDeviceInfoW(deviceInfoSet, className.ToString(), ref classGuid, null, IntPtr.Zero, DICD_GENERATE_ID, ref devInfoData))
            {
                return "SetupDiCreateDeviceInfoW failed: " + Marshal.GetLastWin32Error();
            }

            byte[] hwIdBytes = Encoding.Unicode.GetBytes(hardwareId + "\0\0");
            if (!SetupDiSetDeviceRegistryPropertyW(deviceInfoSet, ref devInfoData, SPDRP_HARDWAREID, hwIdBytes, hwIdBytes.Length))
            {
                return "SetupDiSetDeviceRegistryPropertyW failed: " + Marshal.GetLastWin32Error();
            }

            if (!SetupDiCallClassInstaller(DIF_REGISTERDEVICE, deviceInfoSet, ref devInfoData))
            {
                return "SetupDiCallClassInstaller failed: " + Marshal.GetLastWin32Error();
            }

            bool rebootRequired = false;
            if (!UpdateDriverForPlugAndPlayDevicesW(IntPtr.Zero, hardwareId, infPath, INSTALLFLAG_FORCE, out rebootRequired))
            {
                return "UpdateDriverForPlugAndPlayDevicesW failed: " + Marshal.GetLastWin32Error();
            }

            return "OK (rebootRequired=" + rebootRequired + ")";
        }
        finally
        {
            SetupDiDestroyDeviceInfoList(deviceInfoSet);
        }
    }
}
"@

    Add-Type -TypeDefinition $csharp -Language CSharp
    Write-Log "Installing driver from $infPath for Root\MttVDD..."
    $result = [VddInstaller]::Install($infPath, 'Root\MttVDD')
    Write-Log "Driver install result: $result"
} catch {
    Write-Log "Unhandled error: $($_.Exception.Message)"
}

exit 0
