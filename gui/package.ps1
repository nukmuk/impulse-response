param(
    [ValidateSet('debug', 'release')]
    [string]$Profile = 'release'
)

$ErrorActionPreference = 'Stop'

$guiRoot = [System.IO.Path]::GetFullPath($PSScriptRoot)
$repositoryRoot = [System.IO.Path]::GetFullPath((Join-Path $guiRoot '..'))
$source = Join-Path $guiRoot "target\$Profile"
$destination = Join-Path $guiRoot "target\package-$Profile"
$runtimeList = Join-Path $repositoryRoot 'vendor\windows-rs\crates\libs\reactor-setup\assets\runtime.txt'
$executable = Join-Path $source 'rust-impulse-response-gui.exe'

if (-not (Test-Path -LiteralPath $executable -PathType Leaf)) {
    throw "GUI executable not found: $executable"
}
if (-not (Test-Path -LiteralPath $runtimeList -PathType Leaf)) {
    throw "Windows Reactor runtime list not found: $runtimeList"
}

$destinationFull = [System.IO.Path]::GetFullPath($destination)
$requiredPrefix = $guiRoot.TrimEnd([System.IO.Path]::DirectorySeparatorChar) + [System.IO.Path]::DirectorySeparatorChar
if (-not $destinationFull.StartsWith($requiredPrefix, [System.StringComparison]::OrdinalIgnoreCase) -or
    -not ([System.IO.Path]::GetFileName($destinationFull)).StartsWith('package-', [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "Refusing to replace unexpected package path: $destinationFull"
}

if (Test-Path -LiteralPath $destinationFull) {
    Remove-Item -LiteralPath $destinationFull -Recurse -Force
}
New-Item -ItemType Directory -Path $destinationFull | Out-Null

$runtimeItems = Get-Content -LiteralPath $runtimeList |
    ForEach-Object { $_.Trim() } |
    Where-Object { $_ -and -not $_.StartsWith('#') }

foreach ($item in $runtimeItems) {
    $itemSource = Join-Path $source $item
    if (-not (Test-Path -LiteralPath $itemSource)) {
        throw "Required Windows Reactor runtime item not found: $itemSource"
    }
    Copy-Item -LiteralPath $itemSource -Destination $destinationFull -Recurse -Force
}

$webViewCore = Join-Path $source 'Microsoft.Web.WebView2.Core.dll'
if (-not (Test-Path -LiteralPath $webViewCore -PathType Leaf)) {
    throw "Required WebView2 projection not found: $webViewCore"
}

Copy-Item -LiteralPath $executable -Destination $destinationFull
Copy-Item -LiteralPath $webViewCore -Destination $destinationFull
Copy-Item -LiteralPath (Join-Path $guiRoot 'DISTRIBUTION.md') -Destination (Join-Path $destinationFull 'README.md')

Write-Output $destinationFull
