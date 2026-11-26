# Draws the app icon source and the three tray icons (idle / recording / transcribing).
# Run from the repo root:  powershell -File scripts\make-icons.ps1
# Then regenerate the bundle icons with:  npx tauri icon app-icon.png
param([string]$Root = (Split-Path -Parent $PSScriptRoot))

Add-Type -AssemblyName System.Drawing

function New-MicPath([float]$s) {
    $path = New-Object System.Drawing.Drawing2D.GraphicsPath
    $w = 0.24 * $s; $x = 0.5 * $s - $w / 2; $y = 0.17 * $s; $h = 0.42 * $s
    $path.AddArc($x, $y, $w, $w, 180, 180)
    $path.AddArc($x, $y + $h - $w, $w, $w, 0, 180)
    $path.CloseFigure()
    return $path
}

function Draw-Mic($g, [float]$s, $color) {
    $g.FillPath((New-Object System.Drawing.SolidBrush $color), (New-MicPath $s))
    $pen = New-Object System.Drawing.Pen $color, (0.065 * $s)
    $pen.StartCap = 'Round'; $pen.EndCap = 'Round'
    $g.DrawArc($pen, 0.29 * $s, 0.30 * $s, 0.42 * $s, 0.40 * $s, 0, 180)
    $g.DrawLine($pen, 0.5 * $s, 0.70 * $s, 0.5 * $s, 0.81 * $s)
    $g.DrawLine($pen, 0.38 * $s, 0.81 * $s, 0.62 * $s, 0.81 * $s)
}

function New-Canvas([int]$s) {
    $bmp = New-Object System.Drawing.Bitmap $s, $s
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.SmoothingMode = 'AntiAlias'
    $g.Clear([System.Drawing.Color]::Transparent)
    return @($bmp, $g)
}

# App icon: rounded square with a gradient.
$s = 1024
$bmp, $g = New-Canvas $s
$r = 0.22 * $s
$bg = New-Object System.Drawing.Drawing2D.GraphicsPath
$bg.AddArc(0, 0, $r, $r, 180, 90); $bg.AddArc($s - $r, 0, $r, $r, 270, 90)
$bg.AddArc($s - $r, $s - $r, $r, $r, 0, 90); $bg.AddArc(0, $s - $r, $r, $r, 90, 90); $bg.CloseFigure()
$grad = New-Object System.Drawing.Drawing2D.LinearGradientBrush (New-Object System.Drawing.Point 0, 0), (New-Object System.Drawing.Point $s, $s), ([System.Drawing.Color]::FromArgb(255, 79, 70, 229)), ([System.Drawing.Color]::FromArgb(255, 14, 165, 233))
$g.FillPath($grad, $bg)
Draw-Mic $g $s ([System.Drawing.Color]::White)
$bmp.Save((Join-Path $Root 'app-icon.png'), [System.Drawing.Imaging.ImageFormat]::Png)
$g.Dispose(); $bmp.Dispose()

# Tray icons: coloured disc with a white mic, readable on light and dark taskbars.
$tray = @{ 'tray-idle' = @(79, 70, 229); 'tray-recording' = @(220, 38, 38); 'tray-busy' = @(217, 119, 6) }
foreach ($name in $tray.Keys) {
    $s = 64
    $bmp, $g = New-Canvas $s
    $c = $tray[$name]
    $g.FillEllipse((New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(255, $c[0], $c[1], $c[2]))), 1, 1, $s - 2, $s - 2)
    Draw-Mic $g $s ([System.Drawing.Color]::White)
    $bmp.Save((Join-Path $Root "src-tauri\icons\$name.png"), [System.Drawing.Imaging.ImageFormat]::Png)
    $g.Dispose(); $bmp.Dispose()
}
Write-Output "icons written"
