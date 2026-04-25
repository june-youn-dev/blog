import {
  assertFixtureFetchOutput,
  assertFixtureRenderedOutput,
  permalinksDir,
  postsDir,
  runEleventyBuild,
  runFetchPosts,
  siteOutputDir,
  withFixtureApi,
  withPreservedDirectories,
} from "./fixture-utils.mts";

await withPreservedDirectories([postsDir, permalinksDir, siteOutputDir], async () => {
  await withFixtureApi(async (apiUrl) => {
    await runFetchPosts(apiUrl);
    await assertFixtureFetchOutput();
    await runEleventyBuild();
    await assertFixtureRenderedOutput();
  });
});
