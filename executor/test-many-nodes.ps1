function RunInstance {
    param (
        [String]$Port,
        [String[]]$Seeds
    )
    $env:MU__connection_manager__listen_port = $Port
    $env:MU__gateway_manager__listen_port = $Port
    for ($i = 0; $i -lt $Seeds.Count; $i++) {
        New-Item -Path Env:\MU__gossip__seeds[$($i)]__address -Value '127.0.0.1' -Force
        New-Item -Path Env:\MU__gossip__seeds[$($i)]__port -Value $($Seeds[$i]) -Force
    }
    Start-Process -FilePath cargo -ArgumentList ('run')
}

cargo build
Remove-Item Env:\MU__*

$Seeds = @()
for ($i = 0; $i -lt 3; $i++) {
    $Seeds += , (20000 + $i)
    RunInstance -Port $(20000 + $i) -Seeds $Seeds
}

for ($i = 0; $i -lt 20; $i++) {
    RunInstance -Port $(21000 + $i) -Seeds $Seeds
}

Remove-Item Env:\MU__*