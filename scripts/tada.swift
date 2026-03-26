import AppKit
import Foundation
import QuartzCore

final class TadaApp: NSObject, NSApplicationDelegate {
  private var windows: [NSWindow] = []

  func applicationDidFinishLaunching(_ notification: Notification) {
    NSApp.setActivationPolicy(.accessory)

    let screens = NSScreen.screens
    guard !screens.isEmpty else {
      NSApp.terminate(nil)
      return
    }

    for screen in NSScreen.screens {
      let screenFrame = screen.frame
      windows.append(makeWindow(screenFrame: screenFrame))
    }
    windows.forEach { $0.orderFrontRegardless() }

    DispatchQueue.main.asyncAfter(deadline: .now() + 2.35) {
      self.windows.forEach { $0.close() }
      NSApp.terminate(nil)
    }
  }

  private func makeWindow(screenFrame: NSRect) -> NSWindow {
    let window = NSWindow(
      contentRect: screenFrame,
      styleMask: [.borderless],
      backing: .buffered,
      defer: false
    )
    window.isOpaque = false
    window.backgroundColor = .clear
    window.hasShadow = false
    window.ignoresMouseEvents = true
    window.level = .screenSaver
    window.collectionBehavior = [.canJoinAllSpaces, .stationary, .ignoresCycle]

    let width = screenFrame.width
    let view = NSView(frame: NSRect(x: 0, y: 0, width: screenFrame.width, height: screenFrame.height))
    view.wantsLayer = true
    view.layer = CALayer()
    view.layer?.backgroundColor = NSColor.clear.cgColor
    window.contentView = view

    let emitter = CAEmitterLayer()
    emitter.emitterPosition = CGPoint(x: width / 2, y: 18)
    emitter.emitterSize = CGSize(width: width, height: 2)
    emitter.emitterShape = .line
    emitter.renderMode = .unordered
    emitter.birthRate = 1
    emitter.emitterCells = palette().map(makeCell)
    view.layer?.addSublayer(emitter)

    return window
  }

  private func makeCell(color: NSColor) -> CAEmitterCell {
    let cell = CAEmitterCell()
    cell.birthRate = 24
    cell.lifetime = 3.0
    cell.lifetimeRange = 0.8
    cell.velocity = 420
    cell.velocityRange = 180
    cell.emissionLongitude = .pi / 2
    cell.emissionRange = .pi / 1.8
    cell.yAcceleration = 380
    cell.spin = 3.5
    cell.spinRange = 2.5
    cell.scale = 0.55
    cell.scaleRange = 0.2
    cell.alphaSpeed = -0.35
    cell.contents = confettiImage(color: color).cgImage(forProposedRect: nil, context: nil, hints: nil)
    return cell
  }

  private func palette() -> [NSColor] {
    [
      NSColor.systemPink,
      NSColor.systemYellow,
      NSColor.systemGreen,
      NSColor.systemBlue,
      NSColor.systemOrange,
      NSColor.systemTeal,
    ]
  }

  private func confettiImage(color: NSColor) -> NSImage {
    let size = NSSize(width: 12, height: 18)
    let image = NSImage(size: size)
    image.lockFocus()
    color.setFill()
    let rect = NSRect(x: 0, y: 0, width: size.width, height: size.height)
    NSBezierPath(roundedRect: rect, xRadius: 2, yRadius: 2).fill()
    image.unlockFocus()
    return image
  }
}

let app = NSApplication.shared
let delegate = TadaApp()
app.delegate = delegate
app.run()
