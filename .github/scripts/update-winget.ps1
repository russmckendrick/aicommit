Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Add-StepSummary {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Message
    )

    if ($env:GITHUB_STEP_SUMMARY) {
        $Message | Out-File -FilePath $env:GITHUB_STEP_SUMMARY -Append -Encoding utf8
    }
}

function New-GitHubHeaders {
    $headers = @{
        "User-Agent" = "aicommit-update-winget-workflow"
        "X-GitHub-Api-Version" = "2022-11-28"
    }

    if ($env:GITHUB_TOKEN) {
        $headers.Authorization = "Bearer $env:GITHUB_TOKEN"
    }

    return $headers
}

function Get-RequiredEnv {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Name
    )

    $value = [Environment]::GetEnvironmentVariable($Name)

    if ([string]::IsNullOrWhiteSpace($value)) {
        throw "Missing required environment variable '$Name'."
    }

    return $value
}

function Get-EnvInt {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Name,
        [Parameter(Mandatory = $true)]
        [int]$DefaultValue
    )

    $raw = [Environment]::GetEnvironmentVariable($Name)

    if ([string]::IsNullOrWhiteSpace($raw)) {
        return $DefaultValue
    }

    return [int]$raw
}

function Resolve-ReleaseTag {
    $requestedTag = [Environment]::GetEnvironmentVariable("RELEASE_TAG")

    if (-not [string]::IsNullOrWhiteSpace($requestedTag)) {
        return $requestedTag.Trim()
    }

    if ($env:GITHUB_REF -like "refs/tags/*" -and -not [string]::IsNullOrWhiteSpace($env:GITHUB_REF_NAME)) {
        return $env:GITHUB_REF_NAME.Trim()
    }

    $repository = Get-RequiredEnv -Name "GITHUB_REPOSITORY"
    $headers = New-GitHubHeaders
    $release = Invoke-RestMethod -Headers $headers -Uri "https://api.github.com/repos/$repository/releases/latest"

    if ([string]::IsNullOrWhiteSpace($release.tag_name)) {
        throw "Unable to determine the latest release tag for '$repository'."
    }

    return $release.tag_name.Trim()
}

function Wait-ForReleaseAsset {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Repository,
        [Parameter(Mandatory = $true)]
        [string]$Tag,
        [Parameter(Mandatory = $true)]
        [string]$InstallerName,
        [Parameter(Mandatory = $true)]
        [int]$Attempts,
        [Parameter(Mandatory = $true)]
        [int]$DelaySeconds
    )

    $headers = New-GitHubHeaders
    $releaseUri = "https://api.github.com/repos/$Repository/releases/tags/$Tag"
    $lastError = $null

    for ($attempt = 1; $attempt -le $Attempts; $attempt++) {
        try {
            $release = Invoke-RestMethod -Headers $headers -Uri $releaseUri
            $asset = $release.assets | Where-Object { $_.name -eq $InstallerName } | Select-Object -First 1

            if ($asset -and $asset.state -eq "uploaded" -and -not [string]::IsNullOrWhiteSpace($asset.browser_download_url)) {
                Write-Host "Found uploaded Windows release asset '$InstallerName' for tag '$Tag'."
                return $asset
            }
        } catch {
		$lastError = $_.Exception.Message
        }

        if ($attempt -eq $Attempts) {
		if ($lastError) {
			throw "Windows release asset '$InstallerName' for tag '$Tag' could not be confirmed after $Attempts attempts with a ${DelaySeconds}-second delay. Last error: $lastError"
		}

		throw "Windows release asset '$InstallerName' for tag '$Tag' was not ready after $Attempts attempts with a ${DelaySeconds}-second delay."
        }

        Write-Host "Release asset '$InstallerName' for tag '$Tag' is not ready yet; retrying in $DelaySeconds seconds ($attempt/$Attempts)."
        Start-Sleep -Seconds $DelaySeconds
    }

    throw "Windows release asset '$InstallerName' for tag '$Tag' could not be resolved."
}

function Wait-ForDownloadProbe {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Tag,
        [Parameter(Mandatory = $true)]
        [string]$InstallerName,
        [Parameter(Mandatory = $true)]
        [string]$InstallerUrl,
        [Parameter(Mandatory = $true)]
        [int]$Attempts,
        [Parameter(Mandatory = $true)]
        [int]$DelaySeconds
    )

    for ($attempt = 1; $attempt -le $Attempts; $attempt++) {
        $probePath = Join-Path $env:RUNNER_TEMP "winget-installer-probe-$attempt.bin"
        $downloaded = $false

        try {
            Invoke-WebRequest -MaximumRedirection 5 -Uri $InstallerUrl -OutFile $probePath | Out-Null

            if ((Test-Path $probePath) -and (Get-Item $probePath).Length -gt 0) {
                Remove-Item $probePath -Force
                Write-Host "Verified downloadable Windows release asset '$InstallerName' for tag '$Tag'."
                return
            }

            $downloaded = $true
        } catch {
            if ($attempt -eq $Attempts) {
                throw "Windows release asset '$InstallerName' for tag '$Tag' was not downloadable after $Attempts attempts with a ${DelaySeconds}-second delay: $InstallerUrl"
            }
        } finally {
            if (Test-Path $probePath) {
                Remove-Item $probePath -Force
            }
        }

        if ($attempt -eq $Attempts) {
            if ($downloaded) {
                throw "Windows release asset '$InstallerName' for tag '$Tag' downloaded but was empty after $Attempts attempts with a ${DelaySeconds}-second delay: $InstallerUrl"
            }

            throw "Windows release asset '$InstallerName' for tag '$Tag' was not downloadable after $Attempts attempts with a ${DelaySeconds}-second delay: $InstallerUrl"
        }

        Write-Host "Installer download probe failed for '$InstallerName'; retrying in $DelaySeconds seconds ($attempt/$Attempts)."
        Start-Sleep -Seconds $DelaySeconds
    }
}

function Test-WingetPackageExists {
    param(
        [Parameter(Mandatory = $true)]
        [string]$PackageManifestPath
    )

    $url = "https://api.github.com/repos/microsoft/winget-pkgs/contents/$PackageManifestPath"
    $headers = @{
        "User-Agent" = "aicommit-update-winget-workflow"
        "X-GitHub-Api-Version" = "2022-11-28"
    }

    try {
        Invoke-RestMethod -Headers $headers -Uri $url | Out-Null
        return $true
    } catch {
        $response = $_.Exception.Response
        $statusCode = $null

        if ($response) {
            $statusCode = $response.StatusCode.value__
        }

        if ($statusCode -eq 404) {
            return $false
        }

        throw
    }
}

$repository = Get-RequiredEnv -Name "GITHUB_REPOSITORY"
$installerName = Get-RequiredEnv -Name "INSTALLER_NAME"
$packageIdentifier = Get-RequiredEnv -Name "PACKAGE_IDENTIFIER"
$packageManifestPath = Get-RequiredEnv -Name "PACKAGE_MANIFEST_PATH"
$assetMetadataAttempts = Get-EnvInt -Name "ASSET_METADATA_ATTEMPTS" -DefaultValue 20
$assetMetadataDelaySeconds = Get-EnvInt -Name "ASSET_METADATA_DELAY_SECONDS" -DefaultValue 15
$assetDownloadAttempts = Get-EnvInt -Name "ASSET_DOWNLOAD_ATTEMPTS" -DefaultValue 20
$assetDownloadDelaySeconds = Get-EnvInt -Name "ASSET_DOWNLOAD_DELAY_SECONDS" -DefaultValue 15

$tag = Resolve-ReleaseTag
$version = $tag.TrimStart("v")

Write-Host "Using release tag '$tag' for WinGet submission."

$asset = Wait-ForReleaseAsset `
    -Repository $repository `
    -Tag $tag `
    -InstallerName $installerName `
    -Attempts $assetMetadataAttempts `
    -DelaySeconds $assetMetadataDelaySeconds

$installerUrl = $asset.browser_download_url

Wait-ForDownloadProbe `
    -Tag $tag `
    -InstallerName $installerName `
    -InstallerUrl $installerUrl `
    -Attempts $assetDownloadAttempts `
    -DelaySeconds $assetDownloadDelaySeconds

if (-not (Test-WingetPackageExists -PackageManifestPath $packageManifestPath)) {
    $message = @(
        "WinGet package ID $packageIdentifier is not live in microsoft/winget-pkgs yet.",
        "",
        "Release tag: $tag",
        "Installer asset: $installerName",
        "Once the initial package PR lands, rerun the manual WinGet workflow and provide release tag $tag if needed."
    ) -join [Environment]::NewLine

    Write-Host $message
    Add-StepSummary -Message $message
    exit 0
}

$wingetCreateToken = Get-RequiredEnv -Name "WINGET_CREATE_GITHUB_TOKEN"
$wingetCreatePath = Join-Path $env:RUNNER_TEMP "wingetcreate.exe"

Invoke-WebRequest https://aka.ms/wingetcreate/latest -OutFile $wingetCreatePath

& $wingetCreatePath update $packageIdentifier `
    -u $installerUrl `
    -v $version `
    -t $wingetCreateToken `
    --submit

if ($LASTEXITCODE -ne 0) {
    throw "wingetcreate update failed for '$packageIdentifier' at tag '$tag'."
}

$successMessage = @(
    "Submitted WinGet update for $packageIdentifier.",
    "",
    "Release tag: $tag",
    "Installer asset: $installerName",
    "Installer URL: $installerUrl"
) -join [Environment]::NewLine

Write-Host $successMessage
Add-StepSummary -Message $successMessage
