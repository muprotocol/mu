function RunInstance {
    param (
        [String]$Port,
        [String[]]$Seeds
    )
    $env:MU__connection_manager__listen_port = $Port
    $env:MU__gateway_manager__listen_port = $Port
    for ($i = 0; $i -lt $Seeds.Count; $i++) {
        New-Item -Path Env:\MU__gossip__seeds__$($i)__address -Value '127.0.0.1' -Force
        New-Item -Path Env:\MU__gossip__seeds__$($i)__port -Value $($Seeds[$i]) -Force
    }
    Start-Process -FilePath cargo -ArgumentList ('run')
}

cargo build
Remove-Item Env:\MU__*
$Seeds = @()
for ($i = 0; $i -lt 5; $i++) {
    $Seeds += , (20000 + $i)
    RunInstance -Port $(20000 + $i) -Seeds $Seeds
}
for ($i = 0; $i -lt 5; $i++) {
    RunInstance -Port $(21000 + $i) -Seeds $Seeds
}
