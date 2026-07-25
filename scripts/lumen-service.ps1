<#
.SYNOPSIS
    Manage Lumen Science Server as a Windows Service.
    This is a unique Windows capability — Mac has no equivalent service management.

.DESCRIPTION
    Installs, starts, stops, and removes the Lumen Science HTTP Bridge as a
    native Windows Service. The service auto-starts on boot and restarts on
    failure. Useful for running the science server headlessly.

.PARAMETER Action
    install   — Create and start the Windows service
    start     — Start an existing service
    stop      — Stop a running service
    restart   — Restart the service
    remove    — Remove the service registration
    status    — Show service status

.PARAMETER Port
    Port for the science server. Default: 8420.

.PARAMETER ApiKey
    DEEPSEEK_API_KEY for the service. If omitted, uses existing env var.

.EXAMPLE
    .\lumen-service.ps1 install -Port 8420
    .\lumen-service.ps1 status
    .\lumen-service.ps1 restart
#>

param(
    [ValidateSet("install", "start", "stop", "restart", "remove", "status")]
    [string]$Action = "status",
    [int]$Port = 8420,
    [string]$ApiKey
)

$ErrorActionPreference = "Stop"

$ServiceName = "LumenScience"
$DisplayName = "Lumen Science Server"
$Description = "Lumen AI Science HTTP Bridge — serves the science toolkit over HTTP on port $Port"

$Green  = "$([char]27)[0;32m"
$Yellow = "$([char]27)[1;33m"
$Red    = "$([char]27)[0;31m"
$Bold   = "$([char]27)[1m"
$Reset  = "$([char]27)[0m"

# Ensure running as Administrator for install/remove
$isAdmin = ([Security.Principal.WindowsPrincipal] [Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if ($Action -in @("install", "remove") -and -not $isAdmin) {
    Write-Host "${Red}ERROR: install/remove requires Administrator privileges${Reset}"
    Write-Host "Re-run this script as Administrator."
    exit 1
}

function Get-LumenPath {
    $cmd = Get-Command lumen -ErrorAction SilentlyContinue
    if ($cmd) { return $cmd.Source }

    $commonPaths = @(
        "$env:LOCALAPPDATA\Lumen\lumen.exe",
        "C:\Lumen\lumen.exe",
        "$env:USERPROFILE\lumen\bin\lumen.exe"
    )
    foreach ($p in $commonPaths) {
        if (Test-Path $p) { return $p }
    }
    throw "lumen.exe not found. Install it first with .\lumen-install.ps1"
}

function Invoke-ServiceAction {
    switch ($Action) {
        "install" {
            $lumenPath = Get-LumenPath
            Write-Host "${Bold}Installing Lumen Science Service...${Reset}"

            # Check if already exists
            $existing = Get-Service -Name $ServiceName -ErrorAction SilentlyContinue
            if ($existing) {
                Write-Host "${Yellow}Service already exists. Removing old instance...${Reset}"
                Stop-Service -Name $ServiceName -Force -ErrorAction SilentlyContinue
                sc.exe delete $ServiceName 2>&1 | Out-Null
                Start-Sleep -Seconds 2
            }

            # Set API key
            if ($ApiKey) {
                [Environment]::SetEnvironmentVariable("DEEPSEEK_API_KEY", $ApiKey, "Machine")
                Write-Host "  ${Green}✓${Reset} API key configured at machine level"
            }

            # Create service wrapper script
            $wrapperDir = "$env:ProgramData\Lumen"
            New-Item -ItemType Directory -Force -Path $wrapperDir | Out-Null
            $wrapperScript = Join-Path $wrapperDir "lumen-science-service.ps1"
            @"
# Lumen Science Server wrapper for Windows Service
`$env:DEEPSEEK_API_KEY = [Environment]::GetEnvironmentVariable("DEEPSEEK_API_KEY", "Machine")
`$env:LUMEN_SCIENCE_PORT = "$Port"
& "$lumenPath" science serve --port $Port
"@ | Out-File -FilePath $wrapperScript -Encoding UTF8

            # Create the service
            $pwsh = (Get-Command powershell.exe).Source
            $binaryPath = "$pwsh -ExecutionPolicy Bypass -NoProfile -WindowStyle Hidden -File `"$wrapperScript`""

            New-Service -Name $ServiceName `
                -DisplayName $DisplayName `
                -Description $Description `
                -BinaryPathName $binaryPath `
                -StartupType Automatic

            # Configure recovery: restart on failure
            sc.exe failure $ServiceName reset=86400 actions=restart/5000/restart/10000/restart/30000 2>&1 | Out-Null

            # Configure firewall rule
            $ruleName = "Lumen Science Server (TCP $Port)"
            $existingRule = Get-NetFirewallRule -DisplayName $ruleName -ErrorAction SilentlyContinue
            if (-not $existingRule) {
                New-NetFirewallRule -DisplayName $ruleName `
                    -Direction Inbound -Protocol TCP -LocalPort $Port `
                    -Action Allow -Profile Private,Domain `
                    -Description "Allow inbound connections to Lumen Science Server" | Out-Null
                Write-Host "  ${Green}✓${Reset} Firewall rule created for port $Port"
            }

            Start-Service -Name $ServiceName
            Write-Host "  ${Green}✓${Reset} Service installed and started"
            Write-Host ""
            Write-Host "  Service:  $ServiceName"
            Write-Host "  Status:   $(Get-Service -Name $ServiceName | Select-Object -ExpandProperty Status)"
            Write-Host "  Port:     $Port"
            Write-Host "  Binary:   $lumenPath"
        }

        "start" {
            Start-Service -Name $ServiceName
            Write-Host "${Green}✓${Reset} Service started"
        }

        "stop" {
            Stop-Service -Name $ServiceName -Force
            Write-Host "${Green}✓${Reset} Service stopped"
        }

        "restart" {
            Restart-Service -Name $ServiceName -Force
            Write-Host "${Green}✓${Reset} Service restarted"
        }

        "remove" {
            $svc = Get-Service -Name $ServiceName -ErrorAction SilentlyContinue
            if ($svc) {
                Stop-Service -Name $ServiceName -Force -ErrorAction SilentlyContinue
                sc.exe delete $ServiceName 2>&1 | Out-Null
                Write-Host "${Green}✓${Reset} Service removed"
            } else {
                Write-Host "${Yellow}Service not found — nothing to remove${Reset}"
            }

            # Remove firewall rule
            $ruleName = "Lumen Science Server (TCP $Port)"
            $existingRule = Get-NetFirewallRule -DisplayName $ruleName -ErrorAction SilentlyContinue
            if ($existingRule) {
                Remove-NetFirewallRule -DisplayName $ruleName
                Write-Host "${Green}✓${Reset} Firewall rule removed"
            }
        }

        "status" {
            $svc = Get-Service -Name $ServiceName -ErrorAction SilentlyContinue
            if ($svc) {
                Write-Host "$Bold  Service:$Reset $ServiceName"
                Write-Host "  Status:  $($svc.Status)"
                Write-Host "  Startup: $($svc.StartType)"
                Write-Host "  Binary:  $(Get-LumenPath)"
            } else {
                Write-Host "${Yellow}Service not installed. Run: .\lumen-service.ps1 install${Reset}"
            }
        }
    }
}

try {
    Invoke-ServiceAction
} catch {
    Write-Host "${Red}ERROR: $_${Reset}"
    if ($_.Exception.Message -match "not found") {
        Write-Host "Install lumen first: .\lumen-install.ps1"
    }
    exit 1
}
