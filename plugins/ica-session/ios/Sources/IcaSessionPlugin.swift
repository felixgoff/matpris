import SwiftRs
import Tauri
import UIKit
import WebKit

class MintArgs: Decodable {
  let url: String
  let cookie: String
  let quietMs: Double
  let personMs: Double
  let title: String
}

struct MintedCookie: Encodable {
  let name: String
  let value: String
}

struct Minted: Encodable {
  let cookies: [MintedCookie]
}

/// ICA's WAF cookie, minted the way a shopper's browser gets it: the shop opens in a web
/// view of its own, out of sight, and once the WAF has set its cookie the shop's cookies go
/// back to Rust (src-tauri/src/ica.rs). If ICA wants a person instead, the web view is
/// shown in a sheet for the person using the phone; nothing here answers the check.
class IcaSessionPlugin: Plugin {
  /// One visit at a time (Rust asks one at a time too).
  private var visit: Visit?

  @objc public override func load(webview: WKWebView) {
    // The page keeps itself clear of the notch and home indicator with
    // env(safe-area-inset-*) (viewport-fit=cover); the scroll view's own automatic insets
    // would move it a second time.
    webview.scrollView.contentInsetAdjustmentBehavior = .never
  }

  @objc public func mint(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(MintArgs.self)
    DispatchQueue.main.async {
      if self.visit != nil {
        invoke.reject("ICA's shop is already open")
        return
      }
      let visit = Visit(args: args, host: self.manager.viewController) { cookies, error in
        self.visit = nil
        if let cookies = cookies {
          invoke.resolve(Minted(cookies: cookies))
        } else {
          invoke.reject(error ?? "ICA's check didn't finish")
        }
      }
      self.visit = visit
      visit.start()
    }
  }
}

/// One visit to the shop. Everything here runs on the main thread, as WKWebView requires.
private class Visit: NSObject, UIAdaptivePresentationControllerDelegate {
  private let args: MintArgs
  private weak var host: UIViewController?
  private let done: ([MintedCookie]?, String?) -> Void
  private var webView: WKWebView?
  private var sheet: UIViewController?
  private var timer: Timer?
  private var deadline = Date()
  private var finished = false
  private let shop: URL?

  /// The app's own web view shares this store, as Safari's tabs share one.
  private var jar: WKHTTPCookieStore { WKWebsiteDataStore.default().httpCookieStore }

  init(args: MintArgs, host: UIViewController?, done: @escaping ([MintedCookie]?, String?) -> Void) {
    self.args = args
    self.host = host
    self.done = done
    self.shop = URL(string: args.url)
  }

  func start() {
    guard let shop = shop, let root = host?.view else {
      finish(nil, "can't open ICA's shop")
      return
    }
    forgetToken {
      let config = WKWebViewConfiguration()
      config.websiteDataStore = .default()
      let view = WKWebView(frame: root.bounds, configuration: config)
      view.autoresizingMask = [.flexibleWidth, .flexibleHeight]
      // Behind the app's own web view, so the shop's scripts run as they do on screen (a web
      // view outside the window may not run them), but nobody sees or touches it.
      view.alpha = 0
      view.isUserInteractionEnabled = false
      view.accessibilityElementsHidden = true
      root.insertSubview(view, at: 0)
      self.webView = view
      view.load(URLRequest(url: shop))
      self.deadline = Date().addingTimeInterval(self.args.quietMs / 1000)
      self.timer = Timer.scheduledTimer(withTimeInterval: 0.5, repeats: true) { [weak self] _ in
        self?.poll()
      }
    }
  }

  /// Whether a cookie set for `domain` (".ica.se" or a host) goes to the shop.
  private func sentToShop(_ cookie: HTTPCookie) -> Bool {
    guard let host = shop?.host?.lowercased() else { return false }
    let domain = cookie.domain.lowercased()
    let bare = domain.hasPrefix(".") ? String(domain.dropFirst()) : domain
    let path = shop?.path.isEmpty == false ? shop!.path : "/"
    if let expires = cookie.expiresDate, expires < Date() { return false }
    return (host == bare || host.hasSuffix("." + bare)) && path.hasPrefix(cookie.path)
  }

  /// Deletes any token left from an earlier visit (the store keeps cookies across launches),
  /// so the one we wait for is new.
  private func forgetToken(then: @escaping () -> Void) {
    jar.getAllCookies { cookies in
      let stale = cookies.filter { $0.name == self.args.cookie && self.sentToShop($0) }
      let group = DispatchGroup()
      for cookie in stale {
        group.enter()
        self.jar.delete(cookie) { group.leave() }
      }
      group.notify(queue: .main, execute: then)
    }
  }

  private func poll() {
    if finished { return }
    jar.getAllCookies { cookies in
      if self.finished { return }
      let shopCookies = cookies.filter { self.sentToShop($0) }
      if shopCookies.contains(where: { $0.name == self.args.cookie && !$0.value.isEmpty }) {
        self.finish(shopCookies.map { MintedCookie(name: $0.name, value: $0.value) }, nil)
      } else if Date() >= self.deadline {
        if self.sheet != nil {
          self.finish(nil, "ICA wants a person to answer its check first")
        } else {
          self.askPerson()
        }
      }
    }
  }

  /// ICA's check is for a person: show it to the one using the phone, and wait.
  private func askPerson() {
    guard let view = webView, var top = host else {
      finish(nil, "ICA wants a person to answer its check first")
      return
    }
    while let presented = top.presentedViewController { top = presented }

    view.removeFromSuperview()
    view.alpha = 1
    view.isUserInteractionEnabled = true
    view.accessibilityElementsHidden = false

    let page = UIViewController()
    view.frame = page.view.bounds
    page.view.addSubview(view)
    // The title is long for a navigation bar; two lines rather than cut short.
    let title = UILabel()
    title.text = args.title
    title.font = .preferredFont(forTextStyle: .headline)
    title.numberOfLines = 2
    title.textAlignment = .center
    title.adjustsFontSizeToFitWidth = true
    page.navigationItem.titleView = title
    page.navigationItem.leftBarButtonItem = UIBarButtonItem(
      title: "Not now", style: .plain, target: self, action: #selector(notNow))

    let nav = UINavigationController(rootViewController: page)
    nav.presentationController?.delegate = self
    sheet = nav
    deadline = Date().addingTimeInterval(args.personMs / 1000)
    top.present(nav, animated: true)
  }

  @objc private func notNow() {
    finish(nil, "ICA's check was closed")
  }

  /// Swiped down.
  func presentationControllerDidDismiss(_ presentationController: UIPresentationController) {
    finish(nil, "ICA's check was closed")
  }

  private func finish(_ cookies: [MintedCookie]?, _ error: String?) {
    if finished { return }
    finished = true
    timer?.invalidate()
    timer = nil
    if let sheet = sheet {
      sheet.presentationController?.delegate = nil
      sheet.dismiss(animated: true)
    }
    sheet = nil
    webView?.stopLoading()
    webView?.removeFromSuperview()
    webView = nil
    done(cookies, error)
  }
}

@_cdecl("init_plugin_ica_session")
func initPlugin() -> Plugin {
  return IcaSessionPlugin()
}
