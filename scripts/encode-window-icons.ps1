param(
    [Parameter(Mandatory=$true)][string]$SourcePath,
    [Parameter(Mandatory=$true)][string]$OutputPath
)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
$bytes = [IO.File]::ReadAllBytes((Resolve-Path $SourcePath))
$entries = @()
foreach ($index in 0..([BitConverter]::ToUInt16($bytes,4)-1)) {
    $entryOffset = 6+16*$index
    $width = [int]$bytes[$entryOffset]
    $height = [int]$bytes[$entryOffset+1]
    $length = [BitConverter]::ToUInt32($bytes,$entryOffset+8)
    $offset = [BitConverter]::ToUInt32($bytes,$entryOffset+12)
    $payload = [byte[]]::new($length)
    [Array]::Copy($bytes,$offset,$payload,0,$length)
    if ($width -gt 0 -and $width -le 64) {
        $png = [IO.MemoryStream]::new($payload,$false)
        $bitmap = [Drawing.Bitmap]::new($png)
        $dib = [IO.MemoryStream]::new()
        $writer = [IO.BinaryWriter]::new($dib)
        try {
            if ($bitmap.Width -ne $width -or $bitmap.Height -ne $height) { throw 'Icon dimensions differ from directory' }
            # BITMAPINFOHEADER height includes both the color image and AND mask.
            $writer.Write([uint32]40); $writer.Write([int32]$width); $writer.Write([int32]($height*2))
            $writer.Write([uint16]1); $writer.Write([uint16]32); $writer.Write([uint32]0)
            $writer.Write([uint32]($width*$height*4))
            foreach ($unused in 1..4) { $writer.Write([uint32]0) }
            for ($y=$height-1; $y-ge0; $y--) {
                for ($x=0; $x-lt$width; $x++) {
                    $pixel=$bitmap.GetPixel($x,$y)
                    $writer.Write([byte]$pixel.B); $writer.Write([byte]$pixel.G)
                    $writer.Write([byte]$pixel.R); $writer.Write([byte]$pixel.A)
                }
            }
            $maskStride=[int]([math]::Ceiling($width/32)*4)
            for ($y=$height-1; $y-ge0; $y--) {
                $mask=[byte[]]::new($maskStride)
                for ($x=0; $x-lt$width; $x++) {
                    if ($bitmap.GetPixel($x,$y).A-eq0) {
                        $position=[int][math]::Floor($x/8)
                        $mask[$position]=[byte]($mask[$position]-bor(128-shr($x%8)))
                    }
                }
                $writer.Write($mask)
            }
            $writer.Flush(); $payload=$dib.ToArray()
        } finally { $writer.Dispose(); $dib.Dispose(); $bitmap.Dispose(); $png.Dispose() }
    }
    $entries += [pscustomobject]@{width=$width;height=$height;payload=$payload}
}
$output=[IO.MemoryStream]::new()
$writer=[IO.BinaryWriter]::new($output)
try {
    $writer.Write([uint16]0); $writer.Write([uint16]1); $writer.Write([uint16]$entries.Count)
    $offset=6+16*$entries.Count
    foreach ($entry in $entries) {
        $writer.Write([byte]$entry.width); $writer.Write([byte]$entry.height); $writer.Write([uint16]0)
        $writer.Write([uint16]1); $writer.Write([uint16]32)
        $writer.Write([uint32]$entry.payload.Length); $writer.Write([uint32]$offset)
        $offset += $entry.payload.Length
    }
    foreach ($entry in $entries) { $writer.Write([byte[]]$entry.payload) }
    $writer.Flush()
    [IO.File]::WriteAllBytes([IO.Path]::GetFullPath($OutputPath),$output.ToArray())
} finally { $writer.Dispose(); $output.Dispose() }
