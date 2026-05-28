Strict

// =============================================================================
// Loom/Atlas.bb -- spatial world atlas (zones as nodes, portals as edges)
// =============================================================================
//
// The design's #3 signature surface (README.md): "World atlas - spatial
// overview of every zone, with portals drawn as lines between them."
//
// rcce2 doesn't store world positions for zones (they're a flat list), so
// the atlas layout is DERIVED from the portal-link graph topology via a
// force-directed layout (Fruchterman-Reingold-style spring model).
// Position is recomputed on entry / when nodes are added or removed (cheap
// at our scale -- a few hundred zones tops); cached between frames so
// re-rendering at 60fps doesn't re-solve the graph every frame.
//
// Layout algorithm (simplified FR):
//   For each iteration (up to ATLAS_ITERATIONS, with cooling):
//     - REPULSION: every pair of nodes pushes apart with force k*k/d
//     - ATTRACTION: every edge pulls its endpoints together with d*d/k
//     - Apply accumulated force, clamped to current temperature
//     - Decay temperature
//   Center + normalize to fit viewport.
//
// Activation: Browser exposes a "Card | Atlas" toggle on the Zones tab
// only. When Atlas mode is on, the browser skips its zone card grid and
// asks Atlas::renderAndUpdate to paint the viewport instead.
//
// Architecture: Type with Methods (Atlas owns layout state). Holds a
// Threads reference so node clicks dispatch focus changes consistently
// with the rest of Loom.


// Layout constants
Const ATLAS_ITERATIONS    = 240
Const ATLAS_NODE_R        = 22
Const ATLAS_NODE_PAD      = 6
Const ATLAS_INITIAL_TEMP# = 80.0
Const ATLAS_MIN_TEMP#     = 0.5
Const ATLAS_TEMP_DECAY#   = 0.97
Const ATLAS_VIEWPORT_PAD  = 60


// -----------------------------------------------------------------------------
// AtlasNode -- one zone node. Position + per-iteration force accumulator.
// Allocated by rebuildLayout, freed by clearNodes. Manual lifecycle (no
// EnableGC in Loom modules).
// -----------------------------------------------------------------------------
Type AtlasNode
    Field ZoneHandle%   // Handle(Area)
    Field Label$        // cached A\Name$ for display
    Field X#, Y#        // current world-space position
    Field DX#, DY#      // accumulated displacement this iteration
    Field SpawnCount%   // total defined SpawnActor slots; node size scales
    Field IssueCount%   // # of issues affecting this zone (computed at build)
    Field Outdoors%     // tint hint -- leafy fill for outdoors, stone for indoor
End Type


// -----------------------------------------------------------------------------
// AtlasEdge -- one directed portal link. Allocated alongside nodes; freed
// alongside nodes.
// -----------------------------------------------------------------------------
Type AtlasEdge
    Field FromHandle%
    Field ToHandle%
End Type


// =============================================================================
// Atlas -- spatial zone-graph view.
// =============================================================================
Type Atlas
    Field threads.Threads

    // Layout state. nodeCount lets us know when zones were added/removed
    // and force a rebuild. minX/maxX/minY/maxY are the layout bounding box
    // computed by recenterLayout; render scales these to the viewport rect.
    Field nodeCount%
    Field minX#, minY#, maxX#, maxY#
    Field temperature#

    // Per-iteration accumulators -- kept as Fields rather than Method
    // Locals because BlitzForge Strict rejects re-assigning a Method
    // Local from inside nested For/If blocks.
    Field iterTemp#


    Method create.Atlas(threads.Threads)
        self\threads = threads
        self\nodeCount = 0
        self\minX# = 0.0 : self\minY# = 0.0 : self\maxX# = 1.0 : self\maxY# = 1.0
        self\temperature# = ATLAS_INITIAL_TEMP#
        Return self
    End Method


    // -------------------------------------------------------------------------
    // renderAndUpdate -- per-frame paint + hit-test for the atlas viewport.
    // viewportX/Y/W/H is the rect Browser hands us (i.e. the area BELOW the
    // ribbon/brand/tab/filter strips and ABOVE the bottom footer).
    //
    // If the node pool is empty (first paint, or zones were added/removed
    // and we haven't rebuilt yet), rebuild the layout before drawing.
    // Returns True if a node was clicked this frame (so Browser knows a
    // focus change happened).
    // -------------------------------------------------------------------------
    Method renderAndUpdate%(viewportX%, viewportY%, viewportW%, viewportH%)
        // Detect zone-count change -- a delete or create would invalidate
        // the cached node pool. Cheap O(zones) walk per frame; the layout
        // rebuild itself is the expensive part and only fires on change.
        Local zonesNow% = Atlas::countZones(self)
        If zonesNow <> self\nodeCount
            Atlas::rebuildLayout(self)
        EndIf

        // Refresh per-node IssueCount from the current BrokenRef pool.
        // O(nodes + entries) per frame -- both capped well below 1000.
        // Cheap and lets the issue badges track Issues modal state live
        // without an explicit invalidation hook.
        Atlas::refreshIssueCounts(self)

        // Background -- darker tint than the browser to read as a distinct
        // surface, with a brass border around the viewport rect.
        LoomFill(viewportX, viewportY, viewportW, viewportH, LOOM_STONE_900_R, LOOM_STONE_900_G, LOOM_STONE_900_B)
        LoomBorder(viewportX, viewportY, viewportW, viewportH, LOOM_BRASS_700_R, LOOM_BRASS_700_G, LOOM_BRASS_700_B)

        If self\nodeCount = 0
            LoomTextCentered(viewportX + viewportW / 2, viewportY + viewportH / 2, "No zones yet -- click + New Zone to add one.", LOOM_STONE_300_R, LOOM_STONE_300_G, LOOM_STONE_300_B)
            Return False
        EndIf

        // Title strip
        LoomText(viewportX + 12, viewportY + 8, "ATLAS  |  " + Str(self\nodeCount) + " zones  |  portal links derived from data", LOOM_BRASS_500_R, LOOM_BRASS_500_G, LOOM_BRASS_500_B)

        Local mx% = MouseX()
        Local my% = MouseY()
        Local clicked% = Loom_MouseClicked()

        // Draw edges first so node disks paint on top of them.
        Atlas::drawEdges(self, viewportX, viewportY + 28, viewportW, viewportH - 28)

        // Draw nodes + hit-test.
        Local hit% = Atlas::drawNodes(self, viewportX, viewportY + 28, viewportW, viewportH - 28, mx, my, clicked)
        Return hit
    End Method


    // -------------------------------------------------------------------------
    // rebuildLayout -- drop old pool, allocate one AtlasNode per zone, run
    // ATLAS_ITERATIONS force-directed steps, recenter. Cheap at our scale.
    // -------------------------------------------------------------------------
    Method rebuildLayout()
        Atlas::clearNodes(self)
        Atlas::clearEdges(self)
        self\nodeCount = 0

        // Seed nodes -- circular layout to start (avoids the all-same-
        // position singularity that gives FR a zero-vector problem).
        Local zones% = Atlas::countZones(self)
        If zones = 0 Then Return

        Local theta# = 0.0
        Local arc# = 0.0
        If zones > 0 Then arc# = 6.28318 / Float(zones)
        Local r# = 200.0
        For Ar.Area = Each Area
            Local n.AtlasNode = New AtlasNode()
            n\ZoneHandle = Handle(Ar)
            n\Label = Ar\Name$
            n\X# = Cos#(theta * 57.2958) * r
            n\Y# = Sin#(theta * 57.2958) * r
            n\DX# = 0.0
            n\DY# = 0.0
            // Count defined spawn slots up front so we don't re-scan
            // 1000 indices every frame in drawNodes. Atlas rebuilds on
            // zone add / delete; spawn-count changes within a zone
            // require a manual rebuild (acceptable since the at-a-glance
            // signal is for project structure, not live tuning).
            Local sci% = 0
            Local scnt% = 0
            For sci = 0 To 999
                If Ar\SpawnActor[sci] > 0 Then scnt = scnt + 1
            Next
            n\SpawnCount = scnt
            n\Outdoors = Ar\Outdoors
            n\IssueCount = 0   ; populated by Atlas_RefreshIssueCounts after BrokenRefs rebuild
            theta = theta + arc
            self\nodeCount = self\nodeCount + 1
        Next

        // Seed edges -- one per portal whose target name resolves to a
        // zone. Strings the user typed get a soft fail (no edge) rather
        // than a broken edge.
        For Ar.Area = Each Area
            Local p% = 0
            For p = 0 To 99
                If Ar\PortalLinkArea$[p] <> ""
                    Local toHandle% = Atlas::findZoneHandleByName(self, Ar\PortalLinkArea$[p])
                    If toHandle <> 0
                        Local e.AtlasEdge = New AtlasEdge()
                        e\FromHandle = Handle(Ar)
                        e\ToHandle = toHandle
                    EndIf
                EndIf
            Next
        Next

        // Run the force-directed iterations.
        self\temperature# = ATLAS_INITIAL_TEMP#
        Local iter% = 0
        For iter = 0 To ATLAS_ITERATIONS - 1
            Atlas::layoutStep(self)
            self\temperature# = self\temperature# * ATLAS_TEMP_DECAY#
            If self\temperature# < ATLAS_MIN_TEMP# Then self\temperature# = ATLAS_MIN_TEMP#
        Next

        Atlas::recenterLayout(self)
    End Method


    // -------------------------------------------------------------------------
    // layoutStep -- one Fruchterman-Reingold iteration. Repulsion between
    // every pair, attraction along every edge, displacement clamped to
    // current temperature, applied.
    // -------------------------------------------------------------------------
    Method layoutStep()
        // k = ideal edge length. Bigger k spreads the graph more.
        Local k# = 90.0
        Local k2# = k * k

        // Reset accumulated displacement
        Local n.AtlasNode
        For n = Each AtlasNode
            n\DX# = 0.0
            n\DY# = 0.0
        Next

        // Repulsion -- every pair pushes apart. O(N^2) -- fine for ~hundreds.
        Local n1.AtlasNode
        Local n2.AtlasNode
        For n1 = Each AtlasNode
            For n2 = Each AtlasNode
                If Handle(n1) <> Handle(n2)
                    Local dx# = n1\X# - n2\X#
                    Local dy# = n1\Y# - n2\Y#
                    Local dist# = Sqr#(dx * dx + dy * dy)
                    If dist# < 0.01 Then dist# = 0.01
                    Local force# = k2 / dist#
                    n1\DX# = n1\DX# + (dx / dist#) * force#
                    n1\DY# = n1\DY# + (dy / dist#) * force#
                EndIf
            Next
        Next

        // Attraction -- pull edge endpoints together
        Local e.AtlasEdge
        For e = Each AtlasEdge
            Local na.AtlasNode = Atlas::findNodeByHandle(self, e\FromHandle)
            Local nb.AtlasNode = Atlas::findNodeByHandle(self, e\ToHandle)
            If na <> Null And nb <> Null
                Local dxE# = na\X# - nb\X#
                Local dyE# = na\Y# - nb\Y#
                Local distE# = Sqr#(dxE * dxE + dyE * dyE)
                If distE# < 0.01 Then distE# = 0.01
                Local forceE# = (distE * distE) / k
                na\DX# = na\DX# - (dxE / distE) * forceE
                na\DY# = na\DY# - (dyE / distE) * forceE
                nb\DX# = nb\DX# + (dxE / distE) * forceE
                nb\DY# = nb\DY# + (dyE / distE) * forceE
            EndIf
        Next

        // Apply displacement, clamped by temperature
        Local napp.AtlasNode
        For napp = Each AtlasNode
            Local d# = Sqr#(napp\DX# * napp\DX# + napp\DY# * napp\DY#)
            If d# < 0.01 Then d# = 0.01
            Local scale# = self\temperature#
            If d# < scale# Then scale# = d#
            napp\X# = napp\X# + (napp\DX# / d#) * scale#
            napp\Y# = napp\Y# + (napp\DY# / d#) * scale#
        Next
    End Method


    // -------------------------------------------------------------------------
    // recenterLayout -- compute the bounding box of all nodes so render can
    // normalize them to the viewport rect. Stored on self.
    // -------------------------------------------------------------------------
    Method recenterLayout()
        self\minX# = 999999.0 : self\minY# = 999999.0
        self\maxX# = -999999.0 : self\maxY# = -999999.0
        Local n.AtlasNode
        For n = Each AtlasNode
            If n\X# < self\minX# Then self\minX# = n\X#
            If n\Y# < self\minY# Then self\minY# = n\Y#
            If n\X# > self\maxX# Then self\maxX# = n\X#
            If n\Y# > self\maxY# Then self\maxY# = n\Y#
        Next
        // Avoid divide-by-zero in render's scale calc.
        If self\maxX# - self\minX# < 1.0 Then self\maxX# = self\minX# + 1.0
        If self\maxY# - self\minY# < 1.0 Then self\maxY# = self\minY# + 1.0
    End Method


    // -------------------------------------------------------------------------
    // worldToScreenX / worldToScreenY -- map a node's world-space position
    // into the viewport rect, keeping aspect ratio.
    // -------------------------------------------------------------------------
    Method worldToScreenX%(vx%, vw%, wx#)
        Local span# = self\maxX# - self\minX#
        Local norm# = (wx# - self\minX#) / span#
        Return vx + ATLAS_VIEWPORT_PAD + Int(norm# * Float(vw - ATLAS_VIEWPORT_PAD * 2))
    End Method


    Method worldToScreenY%(vy%, vh%, wy#)
        Local span# = self\maxY# - self\minY#
        Local norm# = (wy# - self\minY#) / span#
        Return vy + ATLAS_VIEWPORT_PAD + Int(norm# * Float(vh - ATLAS_VIEWPORT_PAD * 2))
    End Method


    // -------------------------------------------------------------------------
    // drawEdges -- paint a brass line between each connected pair.
    // -------------------------------------------------------------------------
    Method drawEdges(vx%, vy%, vw%, vh%)
        Color LOOM_BRASS_700_R, LOOM_BRASS_700_G, LOOM_BRASS_700_B
        Local e.AtlasEdge
        For e = Each AtlasEdge
            Local na.AtlasNode = Atlas::findNodeByHandle(self, e\FromHandle)
            Local nb.AtlasNode = Atlas::findNodeByHandle(self, e\ToHandle)
            If na <> Null And nb <> Null
                Local x1% = Atlas::worldToScreenX(self, vx, vw, na\X#)
                Local y1% = Atlas::worldToScreenY(self, vy, vh, na\Y#)
                Local x2% = Atlas::worldToScreenX(self, vx, vw, nb\X#)
                Local y2% = Atlas::worldToScreenY(self, vy, vh, nb\Y#)
                Line x1, y1, x2, y2
            EndIf
        Next
    End Method


    // -------------------------------------------------------------------------
    // drawNodes -- paint each node as a stone disk with a brass ring, label
    // below; hit-test for clicks. Returns True if click landed on a node.
    //
    // Hover highlights with arcane border; the currently focused zone (if
    // any) gets a brass-filled disk so the user can see "I am here."
    // -------------------------------------------------------------------------
    Method drawNodes%(vx%, vy%, vw%, vh%, mx%, my%, clicked%)
        Local hit% = False
        Local n.AtlasNode
        For n = Each AtlasNode
            Local sx% = Atlas::worldToScreenX(self, vx, vw, n\X#)
            Local sy% = Atlas::worldToScreenY(self, vy, vh, n\Y#)

            // Per-node radius scales with spawn density. Small zones
            // stay at the base size (ATLAS_NODE_R); a zone with 100
            // spawns ends up 1.5x as big. sqrt() keeps the visual
            // dynamic range usable -- linear scale would make a 1000-spawn
            // zone obliterate everything else.
            Local r% = ATLAS_NODE_R
            If n\SpawnCount > 0
                Local extra# = Sqr(Float(n\SpawnCount)) * 1.2
                If extra > Float(ATLAS_NODE_R) / 2.0 Then extra = Float(ATLAS_NODE_R) / 2.0
                r = ATLAS_NODE_R + Int(extra)
            EndIf

            Local dx% = mx - sx
            Local dy% = my - sy
            Local dist2% = dx * dx + dy * dy
            Local hovered% = (dist2 < r * r)
            Local focused% = (self\threads\focusKind = "zone" And self\threads\focusID = n\ZoneHandle)

            // Node disk -- approximate circle via successive filled rects.
            // Blitz3D doesn't have a one-shot filled-circle primitive, so
            // for the small radius we use we draw a centered square and
            // round it visually with a 1px ring. Cheap and reads as a node.
            // Drop shadow first for visual lift over the graph edges.
            LoomShadowCard(sx - r, sy - r, r * 2, r * 2)
            If focused = True
                LoomFill(sx - r, sy - r, r * 2, r * 2, LOOM_BRASS_500_R, LOOM_BRASS_500_G, LOOM_BRASS_500_B)
            Else If hovered = True
                LoomFill(sx - r, sy - r, r * 2, r * 2, LOOM_ARCANE_700_R, LOOM_ARCANE_700_G, LOOM_ARCANE_700_B)
            Else
                // Outdoors zones get a slightly warmer/lighter stone fill
                // so designers can scan the map for "is the forest the
                // big green cluster?" without opening each zone.
                If n\Outdoors = True
                    LoomFill(sx - r, sy - r, r * 2, r * 2, LOOM_STONE_500_R, LOOM_STONE_500_G, LOOM_STONE_500_B)
                Else
                    LoomFill(sx - r, sy - r, r * 2, r * 2, LOOM_STONE_700_R, LOOM_STONE_700_G, LOOM_STONE_700_B)
                EndIf
            EndIf

            // Outer brass ring
            LoomBorder(sx - r, sy - r, r * 2, r * 2, LOOM_BRASS_500_R, LOOM_BRASS_500_G, LOOM_BRASS_500_B)

            // Spawn count inside the node -- "12" if non-zero, else
            // blank. Quick "is this zone empty?" cue.
            If n\SpawnCount > 0
                LoomTextCentered(sx, sy - 6, Str(n\SpawnCount), LOOM_PARCHMENT_100_R, LOOM_PARCHMENT_100_G, LOOM_PARCHMENT_100_B)
            EndIf

            // Issue-count badge top-right corner -- only when issues
            // exist. Small danger-red pill with the count. Designers
            // scanning the atlas see broken/dangerous zones immediately.
            If n\IssueCount > 0
                Local badgeW% = 16
                Local badgeX% = sx + r - badgeW
                Local badgeY% = sy - r
                LoomFill(badgeX, badgeY, badgeW, 14, LOOM_DANGER_R, LOOM_DANGER_G, LOOM_DANGER_B)
                LoomBorder(badgeX, badgeY, badgeW, 14, LOOM_PARCHMENT_100_R, LOOM_PARCHMENT_100_G, LOOM_PARCHMENT_100_B)
                LoomTextCentered(badgeX + badgeW / 2, badgeY + 1, Str(n\IssueCount), LOOM_PARCHMENT_100_R, LOOM_PARCHMENT_100_G, LOOM_PARCHMENT_100_B)
            EndIf

            // Label below
            Local labelTxt$ = n\Label
            If Len(labelTxt) > 16 Then labelTxt = Left$(labelTxt, 14) + ".."
            LoomTextCentered(sx, sy + r + 4, labelTxt, LOOM_PARCHMENT_100_R, LOOM_PARCHMENT_100_G, LOOM_PARCHMENT_100_B)

            If hovered = True And clicked = True
                Threads::focus(self\threads, "zone", n\ZoneHandle)
                hit = True
                WriteLog(LoomLog, "Atlas: focused zone handle " + Str(n\ZoneHandle))
            EndIf
        Next
        Return hit
    End Method


    // -------------------------------------------------------------------------
    // refreshIssueCounts -- zero each node's IssueCount, then walk the
    // current BrokenRef pool and increment per-node for any entry whose
    // SourceKind = "zone" and SourceRefID matches a node's ZoneHandle.
    // Cheap O(nodes + entries) -- both capped well under 1000.
    // -------------------------------------------------------------------------
    Method refreshIssueCounts()
        Local n.AtlasNode
        For n = Each AtlasNode
            n\IssueCount = 0
        Next

        Local br.BrokenRef
        For br = Each BrokenRef
            If br\SourceKind = "zone"
                // Linear scan to find matching node -- typically <50 nodes
                // so the inner walk is trivial; keeps refresh simple.
                For n = Each AtlasNode
                    If n\ZoneHandle = br\SourceRefID
                        n\IssueCount = n\IssueCount + 1
                        Exit
                    EndIf
                Next
            EndIf
        Next
    End Method


    // -------------------------------------------------------------------------
    // Helpers
    // -------------------------------------------------------------------------

    Method countZones%()
        Local c% = 0
        For Ar.Area = Each Area
            c = c + 1
        Next
        Return c
    End Method


    Method findZoneHandleByName%(name$)
        If name = "" Then Return 0
        Local upr$ = Upper$(name)
        For Ar.Area = Each Area
            If Upper$(Ar\Name$) = upr Then Return Handle(Ar)
        Next
        Return 0
    End Method


    Method findNodeByHandle.AtlasNode(h%)
        Local n.AtlasNode
        For n = Each AtlasNode
            If n\ZoneHandle = h Then Return n
        Next
        Return Null
    End Method


    Method clearNodes()
        Local n.AtlasNode
        For n = Each AtlasNode
            Delete n
        Next
    End Method


    Method clearEdges()
        Local e.AtlasEdge
        For e = Each AtlasEdge
            Delete e
        Next
    End Method
End Type
