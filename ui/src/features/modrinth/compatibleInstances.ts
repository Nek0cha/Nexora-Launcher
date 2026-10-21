import type { Instance } from "../../bindings/Instance";
import type { LoaderKind } from "../../bindings/LoaderKind";
import type { SearchHit } from "../../bindings/SearchHit";

/** ローダーが読み込める MOD の Modrinth のローダー名。Quilt は Fabric の MOD も読み込める。`vanilla` は空。 */
export function loaderSlugs(loader: LoaderKind): string[] {
  if (loader === "vanilla") return [];
  return loader === "quilt" ? ["quilt", "fabric"] : [loader];
}

/** 検索結果の MOD が入るインスタンス。Minecraft のバージョンが合い、ローダーが一致するもの。 */
export function compatibleInstances(hit: SearchHit, instances: Instance[]): Instance[] {
  return instances.filter((instance) => {
    if (instance.loader === "vanilla") return false;
    if (!hit.versions.includes(instance.mcVersion)) return false;
    const accepted = loaderSlugs(instance.loader);
    return hit.categories.some((category) => accepted.includes(category));
  });
}
