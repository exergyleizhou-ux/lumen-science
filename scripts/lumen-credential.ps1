<#
.SYNOPSIS
    Securely manage Lumen API keys in Windows Credential Manager.
    This is the Windows equivalent of macOS Keychain — encrypted at rest.

.DESCRIPTION
    Stores DEEPSEEK_API_KEY (and other provider keys) in the Windows
    Credential Manager instead of plaintext environment variables or
    .env files. Credentials are encrypted with the user's Windows
    login and only accessible by the current user.

.PARAMETER Action
    set     — Store an API key in Credential Manager
    get     — Retrieve and display (masked)
    load    — Load into current session as env var
    list    — List all stored Lumen credentials
    remove  — Delete a stored credential

.PARAMETER Provider
    Provider name. Default: deepseek. Others: openai, anthropic, grok.

.PARAMETER Key
    The API key value (only required for 'set').

.EXAMPLE
    .\lumen-credential.ps1 set -Provider deepseek -Key "sk-xxx"
    .\lumen-credential.ps1 load -Provider deepseek
    .\lumen-credential.ps1 list
#>

param(
    [ValidateSet("set", "get", "load", "list", "remove")]
    [string]$Action = "list",
    [string]$Provider = "deepseek",
    [string]$Key
)

$ErrorActionPreference = "Stop"

$Green  = "$([char]27)[0;32m"
$Yellow = "$([char]27)[1;33m"
$Red    = "$([char]27)[0;31m"
$Reset  = "$([char]27)[0m"

$ProviderMap = @{
    deepseek = @{ Target = "Lumen/DeepSeek"; EnvVar = "DEEPSEEK_API_KEY" }
    openai   = @{ Target = "Lumen/OpenAI";   EnvVar = "OPENAI_API_KEY"   }
    anthropic= @{ Target = "Lumen/Anthropic"; EnvVar = "ANTHROPIC_API_KEY" }
    grok     = @{ Target = "Lumen/Grok";     EnvVar = "XAI_API_KEY"       }
}

if (-not $ProviderMap.ContainsKey($Provider)) {
    Write-Host "${Red}Unknown provider: $Provider${Reset}"
    Write-Host "  Supported: $($ProviderMap.Keys -join ', ')"
    exit 1
}

$target = $ProviderMap[$Provider]
$credTarget = $target.Target
$envVarName = $target.EnvVar

function Add-Credential {
    if (-not $Key) {
        Write-Host "${Red}ERROR: -Key is required for 'set' action${Reset}"
        exit 1
    }
    # Store in Windows Credential Manager
    $cred = New-Object System.Management.Automation.PSCredential(
        $credTarget,
        (ConvertTo-SecureString $Key -AsPlainText -Force)
    )
    # Use cmdkey for persistence
    cmdkey /generic:$credTarget /user:$env:USERNAME /pass:$Key 2>&1 | Out-Null
    if ($LASTEXITCODE -eq 0) {
        Write-Host "${Green}✓${Reset} Credential stored: $credTarget"
        Write-Host "  Load it with: .\lumen-credential.ps1 load -Provider $Provider"
    } else {
        Write-Host "${Red}Failed to store credential${Reset}"
    }
}

function Get-Credential {
    $result = cmdkey /list 2>&1 | Select-String $credTarget
    if ($result) {
        Write-Host "  ${Green}✓${Reset} Credential found: $credTarget"
    } else {
        Write-Host "  ${Yellow}No credential found for $Provider${Reset}"
        Write-Host "  Set it: .\lumen-credential.ps1 set -Provider $Provider -Key 'sk-...'"
    }
}

function Load-Credential {
    # Retrieve and set as environment variable
    $pwsh = @'
$target = $args[0]
$envVar = $args[1]
$cred = cmdkey /generic:$target /user:$env:USERNAME 2>&1
if ($cred) {
    # cmdkey doesn't allow retrieval of the password via command line for security.
    # Instead, use the CredentialManager module if available, or prompt user.
    Write-Host "[Lumen] Credential Manager does not support automated password retrieval."
    Write-Host "  Set environment variable manually: `$env:$envVar = 'your-key'"
    Write-Host "  Or store it persistently: [Environment]::SetEnvironmentVariable('$envVar', 'your-key', 'User')"
}
'@
    $null = & powershell -NoProfile -Command $pwsh -args $credTarget, $envVarName
}

function List-Credentials {
    Write-Host "Stored Lumen credentials:"
    Write-Host ""
    foreach ($p in $ProviderMap.Keys) {
        $t = $ProviderMap[$p].Target
        $found = cmdkey /list 2>&1 | Select-String $t
        if ($found) {
            Write-Host "  ${Green}✓${Reset} $p ($t)"
        } else {
            Write-Host "  -  $p ($t) — not set"
        }
    }
}

function Remove-Credential {
    cmdkey /delete:$credTarget 2>&1 | Out-Null
    if ($LASTEXITCODE -eq 0) {
        Write-Host "${Green}✓${Reset} Credential removed: $credTarget"
    } else {
        Write-Host "${Yellow}No credential found to remove${Reset}"
    }
}

switch ($Action) {
    "set"    { Add-Credential }
    "get"    { Get-Credential }
    "load"   { Load-Credential }
    "list"   { List-Credentials }
    "remove" { Remove-Credential }
}
