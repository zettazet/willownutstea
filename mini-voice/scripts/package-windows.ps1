$ErrorActionPreference='Stop'
Push-Location (Join-Path $PSScriptRoot '..')
try {
    $env:CMAKE_POLICY_VERSION_MINIMUM='3.5'
    cargo build --locked --release --workspace
    if ($LASTEXITCODE -ne 0) { throw 'Release build failed' }
    $dir=Join-Path (Get-Location) 'dist/windows-x64'
    New-Item -ItemType Directory -Force -Path $dir|Out-Null
    Copy-Item target/release/mini-client.exe,target/release/mini-server.exe,server.example.json,LICENSE,README.md $dir
    Copy-Item target/release/mini-client.exe (Join-Path $dir 'goyvoice.exe') -Force
    Copy-Item docs $dir -Recurse -Force
    Copy-Item docs/PORTABLE-WINDOWS.md (Join-Path $dir 'README.md') -Force
    Copy-Item scripts/benchmark.ps1 $dir
    python scripts/notices.py (Join-Path $dir 'THIRD-PARTY-NOTICES.txt')
    if ($LASTEXITCODE -ne 0) { throw 'License notice collection needs review' }
    Compress-Archive -Force -Path "$dir/*" -DestinationPath dist/mini-voice-windows-x64.zip
    Get-FileHash -Algorithm SHA256 dist/mini-voice-windows-x64.zip | Format-List
} finally { Pop-Location }
