import AppKit
import Foundation
import QuartzCore

final class TadaApp: NSObject, NSApplicationDelegate {
  private var emitters: [CAEmitterLayer] = []
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

    DispatchQueue.main.asyncAfter(deadline: .now() + 1.2) {
      self.emitters.forEach { $0.birthRate = 0 }
    }

    DispatchQueue.main.asyncAfter(deadline: .now() + 5.8) {
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

    let leftEmitter = makeEmitter(origin: CGPoint(x: 0, y: 0), longitude: .pi / 4)
    let rightEmitter = makeEmitter(origin: CGPoint(x: width, y: 0), longitude: .pi * 3 / 4)
    view.layer?.addSublayer(leftEmitter)
    view.layer?.addSublayer(rightEmitter)
    emitters.append(contentsOf: [leftEmitter, rightEmitter])

    return window
  }

  private func makeEmitter(origin: CGPoint, longitude: CGFloat) -> CAEmitterLayer {
    let emitter = CAEmitterLayer()
    emitter.emitterPosition = origin
    emitter.emitterSize = .zero
    emitter.emitterShape = .point
    emitter.renderMode = .unordered
    emitter.birthRate = 1
    emitter.emitterCells = palette().map { color in
      makeCell(color: color, longitude: longitude)
    }
    return emitter
  }

  private func makeCell(color: NSColor, longitude: CGFloat) -> CAEmitterCell {
    let cell = CAEmitterCell()
    cell.birthRate = 24
    cell.lifetime = 4.6
    cell.lifetimeRange = 1.2
    cell.velocity = 730
    cell.velocityRange = 140
    cell.emissionLongitude = longitude
    cell.emissionRange = .pi / 2
    cell.yAcceleration = -260
    cell.spin = 3.5
    cell.spinRange = 2.5
    cell.scale = 0.55
    cell.scaleRange = 0.2
    cell.alphaSpeed = -0.18
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
