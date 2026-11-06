/**
 * publish manifest 的生成逻辑。
 *
 * 这里产出的 manifest 是后续 publish executor 真正消费的“执行计划”：
 * - 要发布哪些包
 * - 发布顺序如何排
 * - 包之间有哪些依赖关系
 * - 每一步最终该执行什么 npm publish 命令
 *
 * 它刻意保持确定性，目的是让执行状态可以持久化并在中断后恢复。
 */
import type {
  ProductBundlePlatformMatrixEntry,
  ProductPublishManifest,
  ProductPublishManifestDependencyLink,
  ProductPublishManifestPackage,
  ProductPublishManifestStep,
  ProductReportChecks,
} from '../types.js';

export function createPublishManifest(input: {
  outputRoot: string;
  packageName: string;
  packageVersion: string;
  binaryName: string;
  distTag: string;
  channel: string;
  registry: string;
  access: 'public';
  productTarball: string;
  bundleRoot: string | null;
  packages: ProductPublishManifestPackage[];
  platformMatrix: ProductBundlePlatformMatrixEntry[];
  checks: ProductReportChecks;
}): ProductPublishManifest {
  // 先生成顺序和依赖图，再派生最终 steps，避免多个地方各自推导 publish 顺序。
  const publishOrder = createPublishOrder(input.packages);
  const dependencyLinks = createDependencyLinks(input.packageName, input.packages);
  const steps = createPublishSteps(
    input.packages,
    publishOrder,
    dependencyLinks,
    input.registry,
    input.access,
  );
  return {
    kind: 'product_publish_manifest',
    mode: 'registry_plan',
    outputRoot: input.outputRoot,
    packageName: input.packageName,
    packageVersion: input.packageVersion,
    binaryName: input.binaryName,
    distTag: input.distTag,
    channel: input.channel,
    productTarball: input.productTarball,
    bundleRoot: input.bundleRoot,
    packages: input.packages,
    platformMatrix: input.platformMatrix,
    publishOrder,
    dependencyLinks,
    steps,
    checks: input.checks,
  };
}

function createPublishOrder(packages: ProductPublishManifestPackage[]): string[] {
  // 发布顺序按角色分层：
  // - 先发平台二进制
  // - 再发官方 CLI wrapper
  // - 最后发最终 product 包
  //
  // 这样后面的依赖关系在 registry 里更容易一次性满足。
  const rank: Record<ProductPublishManifestPackage['role'], number> = {
    platform_binary: 0,
    official_cli: 1,
    product: 2,
  };
  return [...packages]
    .sort((left, right) => {
      const byRole = rank[left.role] - rank[right.role];
      if (byRole !== 0) {
        return byRole;
      }
      return left.name.localeCompare(right.name);
    })
    .map((entry) => entry.name);
}

function createDependencyLinks(
  productPackageName: string,
  packages: ProductPublishManifestPackage[],
): ProductPublishManifestDependencyLink[] {
  // 这里描述的是“包之间的发布后依赖关系”，不是 steps 之间的执行顺序。
  // 执行顺序已经由 `publishOrder` 决定，这里更多是为了让 manifest 更可解释。
  const result: ProductPublishManifestDependencyLink[] = [];

  const officialCli = packages.find((entry) => entry.role === 'official_cli');
  if (officialCli) {
    result.push({
      from: productPackageName,
      to: officialCli.name,
      type: 'dependency',
      field: 'dependencies',
    });
  }

  // 平台二进制挂在官方 CLI 的 optionalDependencies 下。
  for (const platformBinary of packages.filter((entry) => entry.role === 'platform_binary')) {
    if (!officialCli) {
      continue;
    }
    result.push({
      from: officialCli.name,
      to: platformBinary.name,
      type: 'optional_dependency',
      field: 'optionalDependencies',
    });
  }

  return result;
}

function createPublishSteps(
  packages: ProductPublishManifestPackage[],
  publishOrder: string[],
  dependencyLinks: ProductPublishManifestDependencyLink[],
  registry: string,
  access: 'public',
): ProductPublishManifestStep[] {
  // 每个 step 都是“已经可以直接交给执行器”的最终命令快照。
  // 依赖字段 `dependsOn` 不是 npm 命令参数，而是给执行/展示层看的步骤依赖信息。
  return publishOrder
    .map((packageName, index) => {
      const current = packages.find((entry) => entry.name === packageName);
      if (!current) {
        return null;
      }
      return {
        id: `publish-${index + 1}`,
        packageName: current.name,
        role: current.role,
        tarball: current.tarball,
        distTag: current.distTag,
        dependsOn: dependencyLinks
          .filter((entry) => entry.from === current.name)
          .map((entry) => entry.to),
        publishConfig: {
          registry,
          access,
          otpRequired: 'unknown',
          provenance: false,
          dryRun: false,
        },
        command: {
          program: 'npm',
          args: [
            'publish',
            current.tarball,
            '--tag',
            current.distTag,
            '--access',
            access,
            '--registry',
            registry,
          ],
        },
      };
    })
    .filter((entry): entry is ProductPublishManifestStep => entry !== null);
}
