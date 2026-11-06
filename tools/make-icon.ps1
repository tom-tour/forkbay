# Generates assets/icon.ico and assets/icon.png with System.Drawing.
Add-Type -AssemblyName System.Drawing
$assets = Join-Path (Split-Path $PSScriptRoot -Parent) 'assets'

function New-Icon([int]$s) {
    $bmp = New-Object Drawing.Bitmap $s, $s
    $g = [Drawing.Graphics]::FromImage($bmp)
    $g.SmoothingMode = 'AntiAlias'
    $k = $s / 64.0
    $bg = New-Object Drawing.Drawing2D.GraphicsPath
    $r = 15 * $k; $w = $s - 1
    $bg.AddArc(0, 0, $r * 2, $r * 2, 180, 90); $bg.AddArc($w - $r * 2, 0, $r * 2, $r * 2, 270, 90)
    $bg.AddArc($w - $r * 2, $w - $r * 2, $r * 2, $r * 2, 0, 90); $bg.AddArc(0, $w - $r * 2, $r * 2, $r * 2, 90, 90); $bg.CloseFigure()
    $grad = New-Object Drawing.Drawing2D.LinearGradientBrush (New-Object Drawing.Point 0, 0), (New-Object Drawing.Point $s, $s), ([Drawing.Color]::FromArgb(124, 92, 255)), ([Drawing.Color]::FromArgb(34, 211, 238))
    $g.FillPath($grad, $bg)

    # Controller silhouette
    $pad = New-Object Drawing.Drawing2D.GraphicsPath
    $pts = @(21, 21, 43, 21, 51, 24, 55, 36, 56, 46, 50, 50, 42, 41, 22, 41, 14, 50, 8, 46, 9, 36, 13, 24)
    $p = for ($i = 0; $i -lt $pts.Count; $i += 2) { New-Object Drawing.PointF ($pts[$i] * $k), ($pts[$i + 1] * $k) }
    $pad.AddClosedCurve([Drawing.PointF[]]$p, 0.35)
    $g.FillPath([Drawing.Brushes]::White, $pad)

    $ink = New-Object Drawing.SolidBrush ([Drawing.Color]::FromArgb(109, 86, 238))
    if ($s -ge 32) {
        $g.FillEllipse($ink, 40 * $k, 26.800 * $k, 6.400 * $k, 6.400 * $k)
        $pen = New-Object Drawing.Pen $ink, (3.400 * $k)
        $pen.StartCap = 'Round'; $pen.EndCap = 'Round'
        $g.DrawLine($pen, 21 * $k, 25 * $k, 21 * $k, 34 * $k)
        $g.DrawLine($pen, 16.500 * $k, 29.500 * $k, 25.500 * $k, 29.500 * $k)
    }
    $g.Dispose()
    return $bmp
}

$sizes = 16, 24, 32, 48, 64, 128, 256
$pngs = foreach ($s in $sizes) {
    $b = New-Icon $s
    $ms = New-Object IO.MemoryStream
    $b.Save($ms, [Drawing.Imaging.ImageFormat]::Png)
    if ($s -eq 256) { $b.Save((Join-Path $assets 'icon.png'), [Drawing.Imaging.ImageFormat]::Png) }
    $b.Dispose()
    , $ms.ToArray()
}
$out = New-Object IO.MemoryStream
$bw = New-Object IO.BinaryWriter $out
$bw.Write([uint16]0); $bw.Write([uint16]1); $bw.Write([uint16]$sizes.Count)
$offset = 6 + 16 * $sizes.Count
for ($i = 0; $i -lt $sizes.Count; $i++) {
    $s = $sizes[$i]; $len = $pngs[$i].Length
    $bw.Write([byte]($s % 256)); $bw.Write([byte]($s % 256)); $bw.Write([byte]0); $bw.Write([byte]0)
    $bw.Write([uint16]1); $bw.Write([uint16]32); $bw.Write([uint32]$len); $bw.Write([uint32]$offset)
    $offset += $len
}
foreach ($p in $pngs) { $bw.Write($p) }
[IO.File]::WriteAllBytes((Join-Path $assets 'icon.ico'), $out.ToArray())
