import { useQuery } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import IconUser from "~icons/ph/user-circle";
import { getSkin } from "../../api/commands";
import { SKIN_KEY } from "./useAccounts";

/** スキンの画像の高さを調べる。読めなければ null。 */
function useSkinHeight(src: string | null | undefined): number | null {
  const [state, setState] = useState<{ src: string; height: number } | null>(null);
  useEffect(() => {
    if (!src) return;
    let cancelled = false;
    const img = new Image();
    img.onload = () => {
      if (!cancelled) setState({ src, height: img.naturalHeight });
    };
    img.src = src;
    return () => {
      cancelled = true;
    };
  }, [src]);
  return src && state?.src === src ? state.height : null;
}

/** スキンの顔（8,8）と帽子（40,8）を拡大して出す。スキンがなければ既定のアイコン。 */
export function SkinFace({ accountId, size }: { accountId: string | null; size: number }) {
  const { data: skin } = useQuery({
    queryKey: [...SKIN_KEY, accountId],
    queryFn: () => getSkin(accountId as string),
    enabled: accountId !== null,
    staleTime: Number.POSITIVE_INFINITY,
  });
  const height = useSkinHeight(skin);

  if (!skin || height === null) {
    return (
      <span
        aria-hidden
        className="inline-flex shrink-0 items-center justify-center rounded-sm bg-surface-2 text-text-muted"
        style={{ width: size, height: size }}
        data-testid="skin-face-default"
      >
        <IconUser style={{ width: size * 0.7, height: size * 0.7 }} />
      </span>
    );
  }

  // 64 px 幅の画像を size * 8 に拡大するので、画像の 1 px は size / 8 px になる。
  const layer = (x: number) => ({
    position: "absolute" as const,
    inset: 0,
    backgroundImage: `url("${skin}")`,
    backgroundSize: `${size * 8}px auto`,
    backgroundPosition: `-${(x / 8) * size}px -${size}px`,
    imageRendering: "pixelated" as const,
  });
  return (
    <span
      aria-hidden
      className="relative inline-block shrink-0 overflow-hidden rounded-sm"
      style={{ width: size, height: size }}
    >
      <span data-testid="skin-face-base" style={layer(8)} />
      {height >= 64 && <span data-testid="skin-face-hat" style={layer(40)} />}
    </span>
  );
}
