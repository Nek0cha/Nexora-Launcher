import { useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import { SkinViewer } from "skinview3d";
import type { SkinVariant } from "../../bindings/SkinVariant";

const REDUCED_MOTION = "(prefers-reduced-motion: reduce)";

// 自動で回る速さ（rad/s）。ライブラリの既定（1.0）は速すぎるので落とす
const AUTO_ROTATE_SPEED = 0.5;

type Props = {
  /** スキンの PNG（data URL）。スキンが無いアカウントは null で、人形は出さず枠だけにする */
  image: string | null;
  variant: SkinVariant;
  /** 着けるマントの PNG（data URL）。着けていなければ null */
  capeImage: string | null;
};

/** 3D のスキンのプレビュー。ドラッグで回り、親の大きさに合わせる。背景は透明で、親の面の色の上に載る */
export function SkinPreview({ image, variant, capeImage }: Props) {
  const { t } = useTranslation();
  const wrapperRef = useRef<HTMLDivElement>(null);
  const viewerRef = useRef<SkinViewer | null>(null);

  // 人形の部品は 1 度だけ作る。props の変化は下の効果で、作り直さずに反映する
  useEffect(() => {
    const wrapper = wrapperRef.current;
    if (!wrapper) return;
    let viewer: SkinViewer;
    try {
      viewer = new SkinViewer({ renderPaused: document.hidden });
    } catch {
      // WebGL が使えない環境では、枠だけにする
      return;
    }
    viewerRef.current = viewer;
    viewer.canvas.setAttribute("aria-hidden", "true");
    wrapper.appendChild(viewer.canvas);
    // 回すのはドラッグだけ。ホイールでの拡大と、右ドラッグでの移動は止める
    viewer.controls.enableZoom = false;
    viewer.controls.enablePan = false;
    viewer.autoRotateSpeed = AUTO_ROTATE_SPEED;

    // 動きを減らす設定では自動で回さない。設定の切り替えにも追従する
    const media = window.matchMedia(REDUCED_MOTION);
    const syncMotion = () => {
      viewer.autoRotate = !media.matches;
    };
    syncMotion();
    media.addEventListener("change", syncMotion);

    // 画面が隠れている間は描画のループを止める
    const syncVisibility = () => {
      viewer.renderPaused = document.hidden;
    };
    document.addEventListener("visibilitychange", syncVisibility);

    const observer = new ResizeObserver((entries) => {
      const rect = entries[entries.length - 1]?.contentRect;
      if (rect && rect.width > 0 && rect.height > 0) viewer.setSize(rect.width, rect.height);
    });
    observer.observe(wrapper);

    return () => {
      observer.disconnect();
      document.removeEventListener("visibilitychange", syncVisibility);
      media.removeEventListener("change", syncMotion);
      viewerRef.current = null;
      viewer.canvas.remove();
      viewer.dispose();
    };
  }, []);

  useEffect(() => {
    const viewer = viewerRef.current;
    if (!viewer) return;
    if (image === null) {
      viewer.resetSkin();
      return;
    }
    // classic はライブラリでは "default"。読み込めない画像は、前の見た目のままにする
    viewer.loadSkin(image, { model: variant === "slim" ? "slim" : "default" }).catch(() => {});
  }, [image, variant]);

  useEffect(() => {
    const viewer = viewerRef.current;
    if (!viewer) return;
    if (capeImage === null) {
      viewer.resetCape();
      return;
    }
    viewer.loadCape(capeImage).catch(() => {});
  }, [capeImage]);

  return (
    <div
      ref={wrapperRef}
      role="img"
      aria-label={t("skins.preview")}
      className="relative size-full min-h-0 overflow-hidden [&>canvas]:absolute [&>canvas]:top-0 [&>canvas]:left-0 [&>canvas]:block [&>canvas]:cursor-grab [&>canvas]:active:cursor-grabbing"
    />
  );
}
