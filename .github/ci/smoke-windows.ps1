Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'


$work = Join-Path $env:TEMP ([Guid]::NewGuid().ToString())
New-Item -ItemType Directory -Path $work | Out-Null
try {
    Copy-Item -Recurse -LiteralPath (Join-Path $PWD "release") -Destination $work
    $directory = Join-Path $work 'release'
    $marker = Get-Item -LiteralPath (Join-Path $directory 'gha-release.json')
    if ($marker.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Unsafe artifact manifest' }
    $manifest = Get-Content -Raw -LiteralPath $marker.FullName | ConvertFrom-Json
    $commit = & git rev-parse HEAD
    if ($LASTEXITCODE -ne 0) { throw 'Cannot resolve smoke commit' }
    if ($manifest.commit -cne $commit -or $manifest.build_id -cne $env:CI_RUN_ID -or $manifest.pipeline -cne $env:GITHUB_REPOSITORY) { throw 'Artifact build identity mismatch' }
    $names = @($manifest.files.PSObject.Properties.Name)
    if ('codescythe-windows-amd64.exe' -cnotin $names) { throw 'Missing Windows CLI artifact' }
    if (Compare-Object -CaseSensitive @($names + 'gha-release.json') @((Get-ChildItem -LiteralPath $directory).Name)) { throw 'Unexpected artifact files' }
    foreach ($name in $names) {
        if ([IO.Path]::GetFileName($name) -cne $name -or $name -in @('.', '..')) { throw 'Invalid artifact path' }
        $file = Get-Item -LiteralPath (Join-Path $directory $name)
        if ($file.PSIsContainer -or ($file.Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw "Unsafe artifact: $name" }
        if ((Get-FileHash -Algorithm SHA256 -LiteralPath $file.FullName).Hash.ToLowerInvariant() -cne $manifest.files.$name) { throw "Artifact checksum mismatch: $name" }
    }
    Write-Host 'Verified same-build Windows CLI artifact'
    $binary = Join-Path $directory 'codescythe-windows-amd64.exe'
    & $binary --version
    if ($LASTEXITCODE -ne 0) { throw 'CLI version failed' }
    & $binary --help
    if ($LASTEXITCODE -ne 0) { throw 'CLI help failed' }
    $output = & $binary --json -C tests/fixtures/knip-export-basics
    if ($LASTEXITCODE -ne 1 -or -not ($output | Select-String -SimpleMatch '"dangling.ts"')) { throw 'CLI fixture analysis failed' }
    $output | Out-Host
} finally {
    Remove-Item -Recurse -Force -LiteralPath $work
}
