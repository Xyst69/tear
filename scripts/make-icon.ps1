<#
.SYNOPSIS
  Draws the Tear icon (a box with a torn-off strip, see website/favicon.svg)
  at every size Windows uses and writes crates\tear-cli\windows\tear.ico.

.DESCRIPTION
  Each size is drawn directly (not downscaled) so small sizes stay sharp.
  The white body has a thin outline so it stays visible on light menus.
#>
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing

$repo = Split-Path -Parent $PSScriptRoot
$target = Join-Path $repo 'crates\tear-cli\windows\tear.ico'
$sizes = 16, 20, 24, 32, 40, 48, 64, 256

$strip = [Drawing.Color]::FromArgb(255, 0x7B, 0x9C, 0xFF)
$body = [Drawing.Color]::FromArgb(255, 0xF4, 0xF6, 0xF9)
$edge = [Drawing.Color]::FromArgb(255, 0x5B, 0x66, 0x72)

function New-IconPng([int]$size) {
    $bmp = New-Object Drawing.Bitmap $size, $size, ([Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $g = [Drawing.Graphics]::FromImage($bmp)
    $g.SmoothingMode = [Drawing.Drawing2D.SmoothingMode]::AntiAlias
    $g.PixelOffsetMode = [Drawing.Drawing2D.PixelOffsetMode]::HighQuality
    $g.Clear([Drawing.Color]::Transparent)
    # Design grid is 32 units (same coordinates as favicon.svg).
    $k = $size / 32.0
    $g.ScaleTransform($k, $k)

    # Strip: 22 x 5 rounded rectangle at (5, 2.5), rotated -9 degrees about (16, 5).
    $state = $g.Save()
    $m = New-Object Drawing.Drawing2D.Matrix
    $m.RotateAt(-9, (New-Object Drawing.PointF 16, 5))
    $g.MultiplyTransform($m)
    $r = 1.5
    $sp = New-Object Drawing.Drawing2D.GraphicsPath
    $sp.AddArc(5, 2.5, 2 * $r, 2 * $r, 180, 90)
    $sp.AddArc(27 - 2 * $r, 2.5, 2 * $r, 2 * $r, 270, 90)
    $sp.AddArc(27 - 2 * $r, 7.5 - 2 * $r, 2 * $r, 2 * $r, 0, 90)
    $sp.AddArc(5, 7.5 - 2 * $r, 2 * $r, 2 * $r, 90, 90)
    $sp.CloseFigure()
    $g.FillPath((New-Object Drawing.SolidBrush $strip), $sp)
    $g.Restore($state)

    # Body: zigzag (torn) top edge, rounded bottom corners.
    $bp = New-Object Drawing.Drawing2D.GraphicsPath
    $zig = [Drawing.PointF[]]@(
        (New-Object Drawing.PointF 4, 13), (New-Object Drawing.PointF 8, 10),
        (New-Object Drawing.PointF 12, 13), (New-Object Drawing.PointF 16, 10),
        (New-Object Drawing.PointF 20, 13), (New-Object Drawing.PointF 24, 10),
        (New-Object Drawing.PointF 28, 13), (New-Object Drawing.PointF 28, 27.5))
    $bp.AddLines($zig)
    $bp.AddArc(23, 25, 5, 5, 0, 90)
    $bp.AddLine(25.5, 30, 6.5, 30)
    $bp.AddArc(4, 25, 5, 5, 90, 90)
    $bp.CloseFigure()
    $g.FillPath((New-Object Drawing.SolidBrush $body), $bp)
    # Outline at least one device pixel wide.
    $pen = New-Object Drawing.Pen $edge, ([Math]::Max(1.0 / $k, 1.1))
    $pen.LineJoin = [Drawing.Drawing2D.LineJoin]::Round
    $g.DrawPath($pen, $bp)

    $g.Dispose()
    $ms = New-Object IO.MemoryStream
    $bmp.Save($ms, [Drawing.Imaging.ImageFormat]::Png)
    $bmp.Dispose()
    , $ms.ToArray()
}

$images = foreach ($s in $sizes) { , (New-IconPng $s) }

# ICO container with PNG-compressed entries (supported since Windows Vista).
$out = New-Object IO.MemoryStream
$w = New-Object IO.BinaryWriter $out
$w.Write([UInt16]0); $w.Write([UInt16]1); $w.Write([UInt16]$sizes.Count)
$offset = 6 + 16 * $sizes.Count
for ($i = 0; $i -lt $sizes.Count; $i++) {
    $dim = if ($sizes[$i] -ge 256) { 0 } else { $sizes[$i] }
    $w.Write([byte]$dim); $w.Write([byte]$dim); $w.Write([byte]0); $w.Write([byte]0)
    $w.Write([UInt16]1); $w.Write([UInt16]32)
    $w.Write([UInt32]$images[$i].Length); $w.Write([UInt32]$offset)
    $offset += $images[$i].Length
}
foreach ($img in $images) { $w.Write($img) }
$w.Flush()
[IO.File]::WriteAllBytes($target, $out.ToArray())
Write-Host "Wrote $target ($($out.Length) bytes, sizes: $($sizes -join ', '))"
