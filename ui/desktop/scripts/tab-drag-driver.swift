// Real OS-level mouse driver for the tear-off Phase 0 measurement.
//
// WHY THIS AND NOT CDP: `Input.dispatchMouseEvent` hands events straight to a
// renderer's input pipeline, so it would report that pointer capture works
// whether or not the OS actually delivers events across a window boundary — a
// test that cannot fail. CGEventPost at .cghidEventTap injects below the window
// server, so the events are routed exactly as a physical mouse's would be, and
// capture behaviour is the real behaviour.
//
// Coordinates are Quartz global display space: origin TOP-left, y grows down —
// the same space Electron's `getBounds()` reports, and NOT AppKit's bottom-left
// `NSEvent.mouseLocation`. Mixing the two is the classic way to get a driver
// that clicks the wrong thing on a multi-display setup.
//
// Usage: swift drag.swift <fromX> <fromY> <toX> <toY> [steps] [stepDelayMs] [pauseAtMs]
import Cocoa

func post(_ type: CGEventType, _ p: CGPoint, _ button: CGMouseButton = .left) {
    guard let e = CGEvent(mouseEventSource: nil, mouseType: type,
                          mouseCursorPosition: p, mouseButton: button) else { return }
    // A drag must carry the button state; without this the app sees a hover.
    e.post(tap: .cghidEventTap)
}

let a = CommandLine.arguments
guard a.count >= 5,
      let fx = Double(a[1]), let fy = Double(a[2]),
      let tx = Double(a[3]), let ty = Double(a[4]) else {
    print("usage: drag.swift fromX fromY toX toY [steps] [stepDelayMs] [pauseAtMs]")
    exit(2)
}
let steps = a.count > 5 ? Int(a[5]) ?? 40 : 40
let stepDelay = UInt32((a.count > 6 ? Double(a[6]) ?? 12 : 12) * 1000)
let pauseAt = a.count > 7 ? Double(a[7]) ?? 0 : 0

let from = CGPoint(x: fx, y: fy)
let to = CGPoint(x: tx, y: ty)

// Settle the cursor first: a press at a point the OS thinks the mouse is not at
// can be delivered to the wrong window.
post(.mouseMoved, from); usleep(120_000)
post(.leftMouseDown, from); usleep(160_000)

for i in 1...steps {
    let t = Double(i) / Double(steps)
    let p = CGPoint(x: from.x + (to.x - from.x) * t, y: from.y + (to.y - from.y) * t)
    post(.leftMouseDragged, p)
    usleep(stepDelay)
    // Pause mid-path so a preview has time to render and be screenshotted.
    if pauseAt > 0, abs(t - pauseAt) < (0.5 / Double(steps)) { usleep(700_000) }
}
usleep(250_000)
post(.leftMouseUp, to)
print("drag: (\(fx),\(fy)) -> (\(tx),\(ty)) in \(steps) steps")
