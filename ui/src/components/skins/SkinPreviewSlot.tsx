import { Component, lazy, type ReactNode, Suspense } from "react";
import type { SkinVariant } from "../../bindings/SkinVariant";

// 3D のライブラリは大きい（約 130 kB gzip）ので、画面を開いたあとに別の塊として読む
const SkinPreview = lazy(() => import("./SkinPreview").then((m) => ({ default: m.SkinPreview })));

/**
 * 子の描画が失敗したら、何も出さない（空の枠のまま）にする。読み込みに失敗した lazy は失敗のままなので、再試行はしない。
 * プレビューは飾りなので、失敗を画面全体のエラーにしない。
 */
class PreviewBoundary extends Component<{ children: ReactNode }, { failed: boolean }> {
  state = { failed: false };

  static getDerivedStateFromError() {
    return { failed: true };
  }

  componentDidCatch(error: unknown) {
    console.warn("SkinPreview: chunk load failed", error);
  }

  render() {
    return this.state.failed ? null : this.props.children;
  }
}

type Props = {
  image: string | null;
  variant: SkinVariant;
  capeImage: string | null;
};

/** 3D プレビューの枠。読み込み中も失敗したときも、同じ大きさの空の枠（文字なし）のまま */
export function SkinPreviewSlot(props: Props) {
  return (
    <div data-testid="skin-preview-slot" className="h-96 w-64 overflow-hidden rounded-lg bg-surface-1">
      <PreviewBoundary>
        <Suspense fallback={<div data-testid="skin-preview-fallback" className="size-full" />}>
          <SkinPreview {...props} />
        </Suspense>
      </PreviewBoundary>
    </div>
  );
}
