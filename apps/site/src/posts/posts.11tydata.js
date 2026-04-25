export default {
  layout: "post.njk",
  tags: ["posts"],
  permalink: ({ slug }) => `/posts/${slug}/index.html`,
};
