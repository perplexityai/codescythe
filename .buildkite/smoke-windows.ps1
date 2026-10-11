Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Set-SmokeCommit {
    if ($env:BUILDKITE_PULL_REQUEST -and $env:BUILDKITE_PULL_REQUEST -ne 'false') {
        $merge = & buildkite-agent meta-data get github-pr-merge
        if ($LASTEXITCODE -ne 0 -or $merge -cnotmatch '^[0-9a-f]{40}$') { throw 'Missing pinned PR merge SHA' }
        & git fetch --no-tags origin $merge | Out-Host
        if ($LASTEXITCODE -ne 0) { throw 'Cannot fetch pinned PR merge' }
        $head = & git rev-parse 'FETCH_HEAD^2'
        if ($LASTEXITCODE -ne 0 -or $head -cne $env:BUILDKITE_COMMIT) { throw 'Pinned merge does not contain PR head' }
        & git checkout --detach FETCH_HEAD | Out-Host
        if ($LASTEXITCODE -ne 0) { throw 'Cannot check out pinned PR merge' }
    }
}

Set-SmokeCommit
$work = Join-Path $env:TEMP ([Guid]::NewGuid().ToString())
New-Item -ItemType Directory -Path $work | Out-Null
try {
    & buildkite-agent artifact download 'release/*' $work --build $env:BUILDKITE_BUILD_ID --step release-build | Out-Host
    if ($LASTEXITCODE -ne 0) { throw 'Artifact download failed' }
    $directory = Join-Path $work 'release'
    $marker = Get-Item -LiteralPath (Join-Path $directory 'buildkite-release.json')
    if ($marker.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Unsafe artifact manifest' }
    $manifest = Get-Content -Raw -LiteralPath $marker.FullName | ConvertFrom-Json
    $commit = & git rev-parse HEAD
    if ($LASTEXITCODE -ne 0) { throw 'Cannot resolve smoke commit' }
    if ($manifest.commit -cne $commit -or $manifest.build_id -cne $env:BUILDKITE_BUILD_ID -or $manifest.pipeline -cne $env:BUILDKITE_PIPELINE_SLUG) { throw 'Artifact build identity mismatch' }
    $names = @($manifest.files.PSObject.Properties.Name)
    if ('codescythe-windows-amd64.exe' -cnotin $names) { throw 'Missing Windows CLI artifact' }
    if (Compare-Object -CaseSensitive @($names + 'buildkite-release.json') @((Get-ChildItem -LiteralPath $directory).Name)) { throw 'Unexpected artifact files' }
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
