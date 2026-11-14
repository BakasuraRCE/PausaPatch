# Lista las tareas programadas que gestiona PausaPatch y muestra si `schtasks`
# devuelve filas repetidas. Ejecutar en PowerShell (no requiere administrador
# para leer, pero sin elevación algunas tareas pueden no ser visibles).

$prefixes = @(
    '\Microsoft\Windows\InstallService',
    '\Microsoft\Windows\UpdateOrchestrator',
    '\Microsoft\Windows\UpdateAssistant',
    '\Microsoft\Windows\WaaSMedic',
    '\Microsoft\Windows\WindowsUpdate',
    '\Microsoft\WindowsUpdate'
)

function Test-Managed([string] $full) {
    foreach ($prefix in $prefixes) {
        if ($full.StartsWith($prefix, 'OrdinalIgnoreCase')) { return $true }
    }
    return $false
}

Write-Host "=== Get-ScheduledTask ===" -ForegroundColor Cyan
$tasks = @(Get-ScheduledTask | Where-Object {
    Test-Managed ($_.TaskPath + $_.TaskName)
} | Select-Object @{n = 'Ruta'; e = { $_.TaskPath } },
                  @{n = 'Tarea'; e = { $_.TaskName } },
                  @{n = 'Estado'; e = { [string]$_.State } })
$tasks | Sort-Object Ruta, Tarea | Format-Table -AutoSize
"Total (Get-ScheduledTask): $($tasks.Count)"

Write-Host "=== schtasks /Query (crudo) ===" -ForegroundColor Cyan
$names = @(schtasks /Query /FO CSV /NH | ForEach-Object {
    ($_ -split '","')[0].Trim('"')
} | Where-Object { Test-Managed $_ })

"Filas devueltas: $($names.Count)"
$unique = @($names | Select-Object -Unique)
"Únicas:          $($unique.Count)"

$repeated = @($names | Group-Object | Where-Object Count -gt 1)
if ($repeated.Count -gt 0) {
    Write-Host "Filas repetidas:" -ForegroundColor Yellow
    $repeated | Sort-Object Count -Descending | Format-Table Count, Name -AutoSize
} else {
    Write-Host "Sin filas repetidas." -ForegroundColor Green
}

Write-Host "=== Nombres únicos ===" -ForegroundColor Cyan
$unique | Sort-Object
