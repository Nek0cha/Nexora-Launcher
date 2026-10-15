import type { ComponentType, SVGProps } from "react";

export function EmptyState({
  title,
  body,
  icon: Icon,
}: {
  title: string;
  body: string;
  icon?: ComponentType<SVGProps<SVGSVGElement>>;
}) {
  // 中身が届いたときに視線が動かないよう、一覧の左端に揃えて左寄せにする
  return (
    <div className="flex h-full flex-col items-start justify-center gap-3 px-10">
      {Icon && <Icon aria-hidden className="size-8 text-text-muted" />}
      <div className="flex flex-col gap-1.5">
        <h1 className="font-display text-lg font-semibold text-text-strong">{title}</h1>
        <p className="max-w-md text-text-muted">{body}</p>
      </div>
    </div>
  );
}
