import type { ContentKind } from "../../bindings/ContentKind";
import type { Instance } from "../../bindings/Instance";
import type { LoaderKind } from "../../bindings/LoaderKind";
import type { SearchHit } from "../../bindings/SearchHit";

/** ローダーが読み込める MOD の Modrinth のローダー名。Quilt は Fabric の MOD も読み込める。`vanilla` は空。 */
export function loaderSlugs(loader: LoaderKind): string[] {
  if (loader === "vanilla") return [];
  return loader === "quilt" ? ["quilt", "fabric"] : [loader];
}

/** `kind` を入れて動くインスタンスか。リソースパックはどれでもよく、MOD とシェーダーはローダーが要る。 */
export function acceptsKind(instance: Instance, kind: ContentKind): boolean {
  return kind === "resourcepack" || instance.loader !== "vanilla";
}

/**
 * 検索結果が入るインスタンス。Minecraft のバージョンが合うもの。MOD はローダーも一致するもの、
 * シェーダーはローダーのあるもの（シェーダーのローダーは MOD として入るので、ここでは見ない）。
 */
export function compatibleInstances(hit: SearchHit, instances: Instance[], kind: ContentKind = "mod"): Instance[] {
  return instances.filter((instance) => {
    if (!acceptsKind(instance, kind)) return false;
    if (!hit.versions.includes(instance.mcVersion)) return false;
    if (kind !== "mod") return true;
    const accepted = loaderSlugs(instance.loader);
    return hit.categories.some((category) => accepted.includes(category));
  });
}
