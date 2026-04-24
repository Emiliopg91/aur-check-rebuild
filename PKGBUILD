pkgname=aur-check-rebuild
pkgver=1.2.2
pkgrel=5
pkgdesc='Pacman hook to check and launch rebuild of AUR packages affected by updates'
arch=('x86_64')
url='https://github.com/Emiliopg91/aur-check-rebuild'
license=('GPL-2')
depends=(
  'alacritty'
  'pacman'
  'python'
  'python-pip'
  'python-dataclasses-json'
  'python-psutil'
)
source=(
  "git+$url.git#tag=$pkgver-$pkgrel"
)
sha256sums=(
  'SKIP'
)
install=${pkgname}.install

package() {
  cd "$srcdir/aur-check-rebuild"

  install -Dm644 "settings.json" "$pkgdir/usr/share/aur-check-rebuild/settings-default.json"
  install -Dm644 "hooks/zz-aur-check-rebuild.hook" "$pkgdir/usr/share/libalpm/hooks/zz-aur-check-rebuild.hook"
  
  install -Dm755 "scripts/main.py" "$pkgdir/usr/share/libalpm/scripts/aur-check-rebuild/main.py"
  find scripts/helpers -type f -exec install -Dm644 "{}" "$pkgdir/usr/share/libalpm/scripts/aur-check-rebuild/helpers" \;
}