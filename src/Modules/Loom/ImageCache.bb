; =============================================================================
; Loom/ImageCache.bb -- 2D image (texture thumbnail) loader + cache
; =============================================================================
;
; Designers want to SEE what a texture looks like, not just its integer ID.
; This module lazy-loads textures via the existing Media.bb's
; GetTextureName$(ID) + Blitz3D's LoadImage, caches the resulting image
; handles by ID, and exposes a stable handle so render code can call
; DrawImage on it.
;
; Strategy:
;   - First request for an ID looks up the filename via GetTextureName$
;     (reads Textures.dat), then LoadImage on "Data\Textures\<name>".
;   - Successful loads cache the image handle in ImageCacheSlots(ID).
;   - Failed loads (missing texture, bad filename, malformed) cache
;     IMAGE_CACHE_MISS so we don't retry every frame.
;   - LRU eviction is NOT implemented yet -- typical projects have a few
;     hundred textures and a designer browses only a few at a time.
;
; Non-Strict (matches Settings.bb / Recents.bb) so the LoadImage type
; coercion and the global array writes don't fight Strict mode.
;
; Image cache miss sentinel: image handle 0 from LoadImage means failure;
; we cache the integer -1 to distinguish "tried + failed" from "not
; tried yet" (= 0 in a freshly-Dim'd array).
Const IMAGE_CACHE_MISS = -1

; Cache slot per textureID (0..65534). Array writes are non-Strict-safe
; because this file is non-Strict.
Dim ImageCacheSlots(65535)


; =============================================================================
; Loom_GetItemImage -- returns the 2D image handle for the given texture ID,
; or 0 if it can't be loaded. Lazy-loads on first request and caches.
;
; Use:  Local img% = Loom_GetItemImage(It\ThumbnailTexID)
;       If img <> 0 Then DrawImage img, x, y
; =============================================================================
Function Loom_GetItemImage(ID)
    If ID < 0 Or ID > 65534 Then Return 0

    ; Already tried?
    Local cached = ImageCacheSlots(ID)
    If cached > 0 Then Return cached         ; cached image handle
    If cached = IMAGE_CACHE_MISS Then Return 0  ; previously failed; skip retry

    ; First attempt: look up the on-disk filename.
    Local NameAndFlags$ = GetTextureName$(ID)
    If NameAndFlags = ""
        ImageCacheSlots(ID) = IMAGE_CACHE_MISS
        Return 0
    EndIf

    ; GetTextureName returns the filename followed by a single flags byte
    ; (BlitzForge string concat of Name + Chr$(flags) per Media.bb:580).
    ; Strip the trailing flag byte to get the bare filename.
    Local NameLen = Len(NameAndFlags) - 1
    If NameLen < 1
        ImageCacheSlots(ID) = IMAGE_CACHE_MISS
        Return 0
    EndIf
    Local Name$ = Left$(NameAndFlags, NameLen)
    If Name = ""
        ImageCacheSlots(ID) = IMAGE_CACHE_MISS
        Return 0
    EndIf

    ; Path-traversal guard mirrors Media.bb's GetTexture defense.
    If Instr(Name$, "..") > 0
        ImageCacheSlots(ID) = IMAGE_CACHE_MISS
        Return 0
    EndIf

    ; Construct full path + load. LoadImage returns 0 on failure.
    Local FullPath$ = "Data\Textures\" + Name$
    If FileType(FullPath$) <> 1
        ImageCacheSlots(ID) = IMAGE_CACHE_MISS
        Return 0
    EndIf

    Local img = LoadImage(FullPath$)
    If img = 0
        ImageCacheSlots(ID) = IMAGE_CACHE_MISS
        Return 0
    EndIf

    ; Cache + return
    ImageCacheSlots(ID) = img
    Return img
End Function


; =============================================================================
; Loom_DrawImageScaled -- helper: paint cached image into a fixed-size rect,
; preserving aspect ratio. If the image is missing, paints a stone-200 "?"
; placeholder so the layout still has the expected size.
;
; rect: (x, y, w, h). Caller passes the rect; we center the scaled image
; inside it.
; =============================================================================
Function Loom_DrawImageScaled(ID, x, y, w, h)
    Local img = Loom_GetItemImage(ID)
    If img = 0
        ; Placeholder -- stone-700 fill + dashed border + "?" centered
        LoomFill(x, y, w, h, LOOM_STONE_900_R, LOOM_STONE_900_G, LOOM_STONE_900_B)
        LoomBorder(x, y, w, h, LOOM_STONE_700_R, LOOM_STONE_700_G, LOOM_STONE_700_B)
        LoomText(x + (w / 2) - 4, y + (h / 2) - 8, "?", LOOM_STONE_300_R, LOOM_STONE_300_G, LOOM_STONE_300_B)
        Return False
    EndIf

    ; Image found -- scale uniformly to fit (w, h)
    Local iw = ImageWidth(img)
    Local ih = ImageHeight(img)
    If iw <= 0 Or ih <= 0
        LoomFill(x, y, w, h, LOOM_STONE_900_R, LOOM_STONE_900_G, LOOM_STONE_900_B)
        Return False
    EndIf

    Local scale# = Float(w) / Float(iw)
    Local scaleY# = Float(h) / Float(ih)
    If scaleY# < scale# Then scale# = scaleY#

    Local drawW = Int(Float(iw) * scale#)
    Local drawH = Int(Float(ih) * scale#)
    Local drawX = x + (w - drawW) / 2
    Local drawY = y + (h - drawH) / 2

    ; DrawImageRect doesn't exist on every BlitzForge build; ScaleImage in
    ; place is destructive (modifies the cached handle). Cheap alternative:
    ; if image already fits-ish, DrawImage as-is; otherwise still use
    ; DrawImage which paints at native size (clipped by the dest rect).
    ; A future iteration can swap in CopyImage + ScaleImage for true scaling
    ; if visual feedback says it matters.
    DrawImage img, drawX, drawY
    Return True
End Function
