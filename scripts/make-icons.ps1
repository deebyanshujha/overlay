# Draws the OVERLAY mark and writes every icon size Tauri bundles.
#
# The mark is three offset rounded panels — the "layers" the product is named
# for — in the app's own graphite/ice palette. Generated rather than committed
# as binaries so the icon can be re-tuned with the palette.

Add-Type -AssemblyName System.Drawing

$ErrorActionPreference = 'Stop'
$outDir = Join-Path $PSScriptRoot '..\src-tauri\icons'
New-Item -ItemType Directory -Force -Path $outDir | Out-Null

function New-RoundedPath([single]$x, [single]$y, [single]$w, [single]$h, [single]$r) {
    $p = New-Object System.Drawing.Drawing2D.GraphicsPath
    $d = $r * 2
    $p.AddArc($x, $y, $d, $d, 180, 90)
    $p.AddArc($x + $w - $d, $y, $d, $d, 270, 90)
    $p.AddArc($x + $w - $d, $y + $h - $d, $d, $d, 0, 90)
    $p.AddArc($x, $y + $h - $d, $d, $d, 90, 90)
    $p.CloseFigure()
    return $p
}

function New-OverlayBitmap([int]$size) {
    $bmp = New-Object System.Drawing.Bitmap($size, $size, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
    $g.Clear([System.Drawing.Color]::Transparent)

    $s = $size / 1024.0

    # Graphite squircle backdrop with a vertical sheen.
    $plate = New-RoundedPath (72 * $s) (72 * $s) (880 * $s) (880 * $s) (208 * $s)
    $backdrop = New-Object System.Drawing.Drawing2D.LinearGradientBrush(
        (New-Object System.Drawing.Point(0, 0)),
        (New-Object System.Drawing.Point(0, $size)),
        [System.Drawing.Color]::FromArgb(255, 32, 35, 42),
        [System.Drawing.Color]::FromArgb(255, 14, 15, 19))
    $g.FillPath($backdrop, $plate)

    # Hairline border, the same one the UI uses on its cards.
    $edge = New-Object System.Drawing.Pen(
        [System.Drawing.Color]::FromArgb(70, 255, 255, 255), [single](6 * $s))
    $g.DrawPath($edge, $plate)

    # Three stacked panels, back to front, each lighter and more opaque:
    # the depth cue reads even at 16px, where detail disappears.
    $panels = @(
        @{ x = 250; y = 300; w = 470; h = 150; a = 64;  c = @(150, 185, 235) },
        @{ x = 290; y = 435; w = 470; h = 150; a = 130; c = @(185, 212, 248) },
        @{ x = 330; y = 570; w = 470; h = 150; a = 245; c = @(236, 243, 255) }
    )
    foreach ($p in $panels) {
        $path = New-RoundedPath ($p.x * $s) ($p.y * $s) ($p.w * $s) ($p.h * $s) (44 * $s)
        $brush = New-Object System.Drawing.SolidBrush(
            [System.Drawing.Color]::FromArgb($p.a, $p.c[0], $p.c[1], $p.c[2]))
        $g.FillPath($brush, $path)
        $brush.Dispose()
        $path.Dispose()
    }

    $edge.Dispose(); $backdrop.Dispose(); $plate.Dispose(); $g.Dispose()
    return $bmp
}

# Tauri's named PNGs plus the extra sizes Windows picks from in Explorer.
$pngTargets = @{
    'icon.png'       = 1024
    '128x128@2x.png' = 256
    '128x128.png'    = 128
    '32x32.png'      = 32
    'Square30x30Logo.png'   = 30
    'Square44x44Logo.png'   = 44
    'Square71x71Logo.png'   = 71
    'Square89x89Logo.png'   = 89
    'Square107x107Logo.png' = 107
    'Square142x142Logo.png' = 142
    'Square150x150Logo.png' = 150
    'Square284x284Logo.png' = 284
    'Square310x310Logo.png' = 310
    'StoreLogo.png'         = 50
}

foreach ($name in $pngTargets.Keys) {
    $bmp = New-OverlayBitmap $pngTargets[$name]
    $bmp.Save((Join-Path $outDir $name), [System.Drawing.Imaging.ImageFormat]::Png)
    $bmp.Dispose()
}

# --- icon.ico -------------------------------------------------------------
# Built by hand because System.Drawing's Icon.Save cannot write a multi-size,
# PNG-compressed ICO. Windows Vista and later read PNG frames inside an ICO,
# which is how the 256px entry stays small.
$icoSizes = @(16, 24, 32, 48, 64, 128, 256)
$frames = @()
foreach ($s in $icoSizes) {
    $bmp = New-OverlayBitmap $s
    $ms = New-Object System.IO.MemoryStream
    $bmp.Save($ms, [System.Drawing.Imaging.ImageFormat]::Png)
    $frames += [pscustomobject]@{ Size = $s; Bytes = $ms.ToArray() }
    $ms.Dispose(); $bmp.Dispose()
}

$ico = New-Object System.IO.MemoryStream
$w = New-Object System.IO.BinaryWriter($ico)
$w.Write([uint16]0)                 # reserved
$w.Write([uint16]1)                 # type: icon
$w.Write([uint16]$frames.Count)

# Directory entries come first, so every image offset is known up front.
$offset = 6 + (16 * $frames.Count)
foreach ($f in $frames) {
    # 256 is encoded as 0 in the single byte the format allows.
    $dim = if ($f.Size -ge 256) { 0 } else { $f.Size }
    $w.Write([byte]$dim)            # width
    $w.Write([byte]$dim)            # height
    $w.Write([byte]0)               # palette size (0 = truecolour)
    $w.Write([byte]0)               # reserved
    $w.Write([uint16]1)             # colour planes
    $w.Write([uint16]32)            # bits per pixel
    $w.Write([uint32]$f.Bytes.Length)
    $w.Write([uint32]$offset)
    $offset += $f.Bytes.Length
}
foreach ($f in $frames) { $w.Write($f.Bytes) }
$w.Flush()
[System.IO.File]::WriteAllBytes((Join-Path $outDir 'icon.ico'), $ico.ToArray())
$w.Dispose(); $ico.Dispose()

Write-Output "Wrote $($pngTargets.Count) PNGs and a $($frames.Count)-frame icon.ico to $outDir"
