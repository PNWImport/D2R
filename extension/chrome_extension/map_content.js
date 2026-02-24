// =============================================================================
// Map Overlay Content Script
// =============================================================================
// Renders game map overlay as a transparent HTML5 Canvas.
// Decodes run-length encoded collision data and draws walls vs walkable.
// =============================================================================

(function() {
    'use strict';

    let canvas = null;
    let ctx = null;
    let visible = false;
    let currentMapData = null;
    let currentGameState = null;
    let opacity = 0.7;

    const COLORS = {
        wallFill:   'rgba(60, 60, 80, 0.25)',
        walkFill:   'rgba(30, 120, 60, 0.12)',
        exit:       'rgba(0, 255, 100, 0.9)',
        waypoint:   'rgba(0, 150, 255, 0.9)',
        staircase:  'rgba(255, 255, 0, 0.9)',
        shrine:     'rgba(180, 100, 255, 0.8)',
        chest:      'rgba(255, 180, 0, 0.8)',
        player:     'rgba(0, 255, 0, 1.0)',
        superUniq:  'rgba(255, 50, 50, 0.9)',
        quest:      'rgba(255, 100, 200, 0.9)',
        portal:     'rgba(100, 200, 255, 0.9)',
        background: 'rgba(10, 10, 15, 0.12)',
        dirLine:    'rgba(0, 255, 100, 0.3)',
        text:       'rgba(255, 255, 255, 0.7)',
    };

    const POI_STYLES = {
        'Exit':        { color: 'rgba(0, 255, 100, 0.9)',   radius: 5 },
        'Waypoint':    { color: 'rgba(0, 150, 255, 0.9)',   radius: 6 },
        'Staircase':   { color: 'rgba(255, 255, 0, 0.9)',   radius: 5 },
        'Shrine':      { color: 'rgba(180, 100, 255, 0.8)', radius: 3 },
        'Chest':       { color: 'rgba(255, 180, 0, 0.8)',   radius: 3 },
        'SuperUnique': { color: 'rgba(255, 50, 50, 0.9)',   radius: 5 },
        'QuestObject': { color: 'rgba(255, 100, 200, 0.9)', radius: 4 },
        'Portal':      { color: 'rgba(100, 200, 255, 0.9)', radius: 5 },
    };

    function initOverlay() {
        if (canvas) return;
        canvas = document.createElement('canvas');
        canvas.id = 'map-overlay';
        canvas.width = 300;
        canvas.height = 300;
        canvas.style.cssText = [
            'position: fixed', 'top: 10px', 'right: 10px',
            'width: 300px', 'height: 300px',
            'pointer-events: none', 'z-index: 2147483647',
            'border-radius: 4px', 'display: none',
            'opacity: ' + opacity,
        ].join('; ');
        document.body.appendChild(canvas);
        ctx = canvas.getContext('2d');
    }

    function showOverlay()  { initOverlay(); canvas.style.display = 'block'; visible = true; }
    function hideOverlay()  { if (canvas) canvas.style.display = 'none'; visible = false; }
    function setOpacity(v)  { opacity = Math.max(0.05, v / 255); if (canvas) canvas.style.opacity = opacity; }

    // ---- Run-Length Decode + Render Collision Grid ----

    function renderCollision(rows, mapW, mapH, scale, ox, oy) {
        if (!rows || !rows.length) return;

        // Use ImageData for pixel-level rendering (faster than fillRect per cell)
        const imgW = Math.ceil(mapW * scale);
        const imgH = Math.ceil(mapH * scale);
        if (imgW <= 0 || imgH <= 0) return;

        const imgData = ctx.createImageData(imgW, imgH);
        const pixels = imgData.data;

        // Wall color: rgba(60, 60, 80, 64)
        const WR = 60, WG = 60, WB = 80, WA = 64;
        // Walk color: rgba(30, 120, 60, 30)
        const FR = 30, FG = 120, FB = 60, FA = 30;

        for (let row = 0; row < rows.length && row < mapH; row++) {
            const rle = rows[row];
            if (!rle || !rle.length) continue;

            // Decode RLE: alternating wall/open, starts with wall
            let mapX = 0;
            let isWall = true;
            for (let i = 0; i < rle.length; i++) {
                const runLen = rle[i];
                const endX = Math.min(mapX + runLen, mapW);

                // Scale to image pixels
                const py0 = Math.floor(row * scale);
                const py1 = Math.max(py0 + 1, Math.ceil((row + 1) * scale));
                const px0 = Math.floor(mapX * scale);
                const px1 = Math.ceil(endX * scale);

                const r = isWall ? WR : FR;
                const g = isWall ? WG : FG;
                const b = isWall ? WB : FB;
                const a = isWall ? WA : FA;

                for (let py = py0; py < py1 && py < imgH; py++) {
                    for (let px = px0; px < px1 && px < imgW; px++) {
                        const idx = (py * imgW + px) * 4;
                        pixels[idx]     = r;
                        pixels[idx + 1] = g;
                        pixels[idx + 2] = b;
                        pixels[idx + 3] = a;
                    }
                }

                mapX = endX;
                isWall = !isWall;
            }
        }

        ctx.putImageData(imgData, Math.floor(ox), Math.floor(oy));
    }

    // ---- Main Render ----

    function renderMap(gameState, mapData) {
        if (!ctx || !mapData) return;
        currentGameState = gameState;
        currentMapData = mapData;

        const w = canvas.width;
        const h = canvas.height;
        ctx.clearRect(0, 0, w, h);

        // Background
        ctx.fillStyle = COLORS.background;
        ctx.fillRect(0, 0, w, h);

        const mapW = mapData.width || 200;
        const mapH = mapData.collision_row_count || mapData.height || 200;
        const scale = Math.min(w / mapW, h / mapH) * 0.9;
        const ox = (w - mapW * scale) / 2;
        const oy = (h - mapH * scale) / 2;

        // Collision grid (RLE decoded)
        if (mapData.collision_rows && Array.isArray(mapData.collision_rows)) {
            renderCollision(mapData.collision_rows, mapW, mapH, scale, ox, oy);
        } else {
            ctx.fillStyle = COLORS.wallFill;
            ctx.fillRect(ox, oy, mapW * scale, mapH * scale);
        }

        // POIs
        if (mapData.pois) {
            for (const poi of mapData.pois) {
                const px = ox + (poi.x - (mapData.origin_x || 0)) * scale;
                const py = oy + (poi.y - (mapData.origin_y || 0)) * scale;
                const style = POI_STYLES[poi.poi_type] || POI_STYLES['Exit'];

                ctx.beginPath();
                ctx.arc(px, py, style.radius, 0, Math.PI * 2);
                ctx.fillStyle = style.color;
                ctx.fill();

                if (poi.label && style.radius >= 4) {
                    ctx.font = '9px sans-serif';
                    ctx.fillStyle = COLORS.text;
                    ctx.textAlign = 'center';
                    ctx.fillText(poi.label, px, py - style.radius - 2);
                }
            }
        }

        // Player dot
        if (gameState && gameState.player_x != null && gameState.player_y != null) {
            const px = ox + (gameState.player_x - (mapData.origin_x || 0)) * scale;
            const py = oy + (gameState.player_y - (mapData.origin_y || 0)) * scale;

            // Pulsing dot + glow
            const pulse = 3 + Math.sin(Date.now() / 200) * 1.5;
            ctx.beginPath();
            ctx.arc(px, py, pulse, 0, Math.PI * 2);
            ctx.fillStyle = COLORS.player;
            ctx.fill();
            ctx.beginPath();
            ctx.arc(px, py, pulse + 2, 0, Math.PI * 2);
            ctx.strokeStyle = 'rgba(0, 255, 0, 0.3)';
            ctx.lineWidth = 1;
            ctx.stroke();

            // Direction line to nearest exit/staircase
            if (mapData.pois) {
                const targets = mapData.pois.filter(p =>
                    p.poi_type === 'Exit' || p.poi_type === 'Staircase'
                );
                if (targets.length > 0) {
                    let best = targets[0], bestD = Infinity;
                    for (const t of targets) {
                        const dx = t.x - gameState.player_x;
                        const dy = t.y - gameState.player_y;
                        const d = dx * dx + dy * dy;
                        if (d < bestD) { bestD = d; best = t; }
                    }
                    const ex = ox + (best.x - (mapData.origin_x || 0)) * scale;
                    const ey = oy + (best.y - (mapData.origin_y || 0)) * scale;

                    ctx.beginPath();
                    ctx.moveTo(px, py);
                    ctx.lineTo(ex, ey);
                    ctx.strokeStyle = COLORS.dirLine;
                    ctx.lineWidth = 1;
                    ctx.setLineDash([4, 4]);
                    ctx.stroke();
                    ctx.setLineDash([]);
                }
            }
        }

        // HUD
        if (gameState) {
            const diff = ['Norm', 'NM', 'Hell'][gameState.difficulty] || '?';
            ctx.font = 'bold 10px sans-serif';
            ctx.fillStyle = COLORS.text;
            ctx.textAlign = 'left';
            ctx.fillText(gameState.area_name + ' (' + diff + ')', 5, h - 5);
            ctx.textAlign = 'right';
            ctx.fillText('Seed: ' + (gameState.map_seed != null ? gameState.map_seed.toString(16).toUpperCase() : '?'), w - 5, h - 5);
        }
    }

    // ---- Message Handler ----

    chrome.runtime.onMessage.addListener((message) => {
        switch (message.type) {
            case 'MAP_UPDATE':
                showOverlay();
                setOpacity(message.opacity || 180);
                renderMap(message.gameState, message.mapData);
                break;
            case 'MAP_RENDER':
                showOverlay();
                renderMap(null, message.mapData);
                break;
            case 'MAP_HIDE':
                hideOverlay();
                break;
        }
    });

    // Pulsing animation
    function animLoop() {
        if (visible && currentGameState && currentMapData) {
            renderMap(currentGameState, currentMapData);
        }
        requestAnimationFrame(animLoop);
    }
    requestAnimationFrame(animLoop);

})();
