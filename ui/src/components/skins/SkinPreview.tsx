import { useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import { SkinViewer } from "skinview3d";
import type { SkinVariant } from "../../bindings/SkinVariant";

const REDUCED_MOTION = "(prefers-reduced-motion: reduce)";

// 自動で回る速さ（rad/s）。ライブラリの既定（1.0）は速すぎるので落とす
const AUTO_ROTATE_SPEED = 0.5;

/**
 * 読み込みを 1 本ずつに直列化する入れ物。ライブラリの loadSkin / loadCape は画像の読み込みを待ってから
 * 内部で反映するので、続けて呼ぶと遅い古い読み込みが新しいものを上書きしうる。
 * 前の読み込みが終わってから次を始め、待っている間に来た要求は最新の 1 つだけ残す（途中のものは捨てる）。
 * 取り消し（reset）も同じ列に乗せ、遅れて終わった読み込みが reset を上書きしないようにする。
 * alive が false になったら（部品が破棄されたら）、次の読み込みは始めない。
 */
function createSerialRunner<T>(run: (value: T) => Promise<void> | void, alive: () => boolean, tag: string) {
  let pending: { value: T } | null = null;
  let busy = false;
  const pump = async () => {
    if (busy) return;
    busy = true;
    while (pending && alive()) {
      const { value } = pending;
      pending = null;
      try {
        await run(value);
      } catch (e) {
        // 読み込めない画像は、前の見た目のままにする
        console.warn(tag, e);
      }
    }
    busy = false;
  };
  return (value: T) => {
    pending = { value };
    void pump();
  };
}

type SkinRequest = { image: string; variant: SkinVariant } | null;

type Runners = {
  skin: (value: SkinRequest) => void;
  cape: (value: string | null) => void;
};

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
  // 今の SkinViewer 用の直列化の入れ物。viewer を作り直すたびに作り直す
  const runnersRef = useRef<Runners | null>(null);

  // 人形の部品は 1 度だけ作る。props の変化は下の効果で、作り直さずに反映する
  useEffect(() => {
    const wrapper = wrapperRef.current;
    if (!wrapper) return;
    let viewer: SkinViewer;
    try {
      viewer = new SkinViewer({ renderPaused: document.hidden });
    } catch (e) {
      // WebGL が使えない環境では、枠だけにする
      console.warn("SkinPreview: viewer create failed", e);
      return;
    }
    // 破棄したあとに終わった読み込みから、viewer を触らせない
    let disposed = false;
    const alive = () => !disposed;
    runnersRef.current = {
      skin: createSerialRunner(
        (request: SkinRequest) => {
          if (request === null) return viewer.resetSkin();
          // classic はライブラリでは "default"
          return viewer.loadSkin(request.image, { model: request.variant === "slim" ? "slim" : "default" });
        },
        alive,
        "SkinPreview: skin load failed",
      ),
      cape: createSerialRunner(
        (capeImage: string | null) => (capeImage === null ? viewer.resetCape() : viewer.loadCape(capeImage)),
        alive,
        "SkinPreview: cape load failed",
      ),
    };
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
      disposed = true;
      runnersRef.current = null;
      viewer.canvas.remove();
      viewer.dispose();
    };
  }, []);

  useEffect(() => {
    runnersRef.current?.skin(image === null ? null : { image, variant });
  }, [image, variant]);

  useEffect(() => {
    runnersRef.current?.cape(capeImage);
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
