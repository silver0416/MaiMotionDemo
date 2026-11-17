#!/bin/bash
# 在乾淨的 Ubuntu 容器裡檢查 AppImage（release.yml 的「Check AppImage」用）：
# 1. 打得開：用 Xvfb 實際開 25 秒，WebKit 的畫面程序要還在，而且沒有因為 EGL 錯誤中止。
#    容器沒有 GPU，Mesa 用軟體繪圖；Ubuntu 26.04 上的中止發生在載入 Mesa 時，和有沒有 GPU 無關。
# 2. 播得了影片：把 AppImage 解開，用 media-check 取代主程式後經由 AppRun 執行，
#    以 AppImage 內附的 WebKit、GStreamer 與外掛跑 media.html（見 media-check.c）。
#    必要的只有 App 在 Linux 的做法（blob: 網址配 preload="metadata"），另外兩種做法只記錄結果。
#
# 用法：docker run --rm -v <資料夾>:/work -w /work ubuntu:26.04 bash /work/check.sh
# <資料夾> 裡要有 app.AppImage、media-check（media-check.c 編好的程式），
# 以及 media/（media.html 與測試影片）。
set -eu

export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
# 一般桌面會有的系統套件：WebKitGTK（連帶 GStreamer）、Mesa 與 ALSA。
# ALSA 在 AppImage 的排除清單上（每台桌面都有），H.264 解碼外掛（libav）要用到；24.04 起改名 libasound2t64。
alsa=libasound2
if apt-cache show libasound2t64 > /dev/null 2>&1; then
  alsa=libasound2t64
fi
apt-get install -y -qq --no-install-recommends \
  libwebkit2gtk-4.1-0 libegl-mesa0 libgl1-mesa-dri "$alsa" xvfb xauth > /dev/null

echo "=== start the app"
APPIMAGE_EXTRACT_AND_RUN=1 xvfb-run -a ./app.AppImage > /tmp/app.log 2>&1 &
sleep 25
cat /tmp/app.log
if grep -q 'EGL_BAD\|Aborting' /tmp/app.log; then
  echo "::error::WebKit aborted"
  exit 1
fi
processes=$(for p in /proc/[0-9]*; do readlink "$p/exe" 2> /dev/null || true; done)
if ! grep -q WebKitWebProcess <<< "$processes"; then
  echo "::error::the app did not start"
  exit 1
fi

echo "=== play videos"
cd /tmp
/work/app.AppImage --appimage-extract > /dev/null
cat squashfs-root/AppRun
ls squashfs-root/usr/lib/gstreamer-1.0 2> /dev/null | tr '\n' ' ' || true
echo
# 外掛缺哪些系統程式庫（應該只剩桌面一定有的；這裡列出來方便查）
missing=$(LD_LIBRARY_PATH=/tmp/squashfs-root/usr/lib ldd squashfs-root/usr/lib/gstreamer-1.0/*.so 2> /dev/null \
  | grep 'not found' | sort -u || true)
echo "libraries not found: ${missing:-none}"
exe=$(sed -n 's/^Exec=\([^ ]*\).*/\1/p' squashfs-root/*.desktop | head -1)
cp /work/media-check "squashfs-root/usr/bin/$exe"
# media.html 的每種載入方式各跑一次。app 是 App 在 Linux 的做法：每一項都要通過，而且 GStreamer
# 不能回報讀到壞資料（H.264 解出錯誤、mp4 結構錯亂、VP9 解不出畫面）；其他方式只記錄結果。
corrupt='Invalid NAL unit size|bogus size|file is invalid|Failed to decode frame'
failed=0
for variant in app blob-auto asset; do
  echo "=== play videos: $variant"
  log="/tmp/media-$variant.log"
  status=0
  APPDIR=/tmp/squashfs-root GST_DEBUG=2 GST_DEBUG_NO_COLOR=1 \
    timeout 240 xvfb-run -a squashfs-root/AppRun /work/media "$variant" > "$log" 2>&1 || status=$?
  errors=$(grep -cE "$corrupt" "$log" || true)
  if [ "$variant" = app ]; then
    cat "$log"
    if [ "$status" -ne 0 ] || [ "$errors" -ne 0 ]; then
      echo "::error::video check failed (exit $status, $errors bad-data errors from GStreamer)"
      failed=1
    fi
  else
    grep -E '^(PASS|FAIL|INFO) ' "$log" || true
    echo "(for information only: exit $status, $errors bad-data errors from GStreamer)"
  fi
done
exit "$failed"
