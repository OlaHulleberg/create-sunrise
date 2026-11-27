package dev.createfuzz;

import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import com.simibubi.create.AllBlocks;
import com.simibubi.create.AllRecipeTypes;
import com.simibubi.create.content.kinetics.deployer.DeployerBlockEntity;
import com.simibubi.create.content.kinetics.deployer.DeployerFakePlayer;
import com.simibubi.create.content.kinetics.deployer.ItemApplicationRecipe;
import com.simibubi.create.content.kinetics.deployer.ManualApplicationRecipe;
import com.simibubi.create.content.processing.recipe.ProcessingRecipe;
import net.minecraft.commands.arguments.blocks.BlockStateParser;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.InteractionResult;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.context.UseOnContext;
import net.minecraft.world.item.crafting.Ingredient;
import net.minecraft.world.item.crafting.RecipeHolder;
import net.minecraft.world.level.ClipContext;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.properties.Property;
import net.minecraft.world.phys.BlockHitResult;
import net.minecraft.world.phys.Vec3;

import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * Checks the deployers' contract (`FUZZ_DEPLOYER_MANIFEST`) against the loaded game at startup, by
 * experiment in a probe area: what the fake player of a deployer does with each held stack on
 * each block (`getDestroySpeed`, `canHarvestBlock`, `getDrops`, `useOn`), and
 * `DeployerBlockEntity.getRecipe` for every pair of the contract's items.
 */
final class DeployerAudit {
    /** An empty hand, as the manifest names it. */
    static final String HAND = "minecraft:air";

    static void check(ServerLevel level) {
        JsonObject doc;
        try {
            doc = JsonParser
                    .parseString(Files.readString(Path.of(System.getenv("FUZZ_DEPLOYER_MANIFEST"))))
                    .getAsJsonObject();
        } catch (java.io.IOException e) {
            throw new IllegalStateException(e);
        }
        BlockPos origin = CreateFuzz.slot(CreateFuzz.SLOTS - 1).offset(12, 8, 12);
        try {
            audit(level, doc, origin);
        } finally {
            CaseIsolation.clear(level, origin);
            for (Entity entity : level.getAllEntities())
                if (!(entity instanceof Player)) entity.discard();
        }
    }

    static void audit(ServerLevel level, JsonObject doc, BlockPos o) {
        Map<String, JsonObject> items = new LinkedHashMap<>();
        for (JsonElement e : doc.getAsJsonArray("items")) {
            JsonObject i = e.getAsJsonObject();
            String id = i.get("id").getAsString();
            items.put(id, i);
            if (!BuiltInRegistries.ITEM.containsKey(ResourceLocation.parse(id)))
                throw new IllegalStateException("deployer contract: no item " + id);
            ItemStack stack = item(id, 1);
            expect("max stack of " + id, i.get("max_stack").getAsInt(), stack.getMaxStackSize());
            expect("max damage of " + id, i.has("max_damage") ? i.get("max_damage").getAsInt() : 0,
                    stack.getMaxDamage());
            ItemStack remainder = stack.getCraftingRemainingItem();
            expect("crafting remainder of " + id, text(i, "remainder"),
                    remainder.isEmpty() ? null : key(remainder.getItem()));
        }
        List<String> states = doc.getAsJsonArray("states").asList().stream().map(JsonElement::getAsString)
                .toList();

        level.setBlock(o.south(4), AllBlocks.DEPLOYER.get().defaultBlockState(), 3);
        DeployerBlockEntity deployer = (DeployerBlockEntity) level.getBlockEntity(o.south(4));
        deployer.initialize();
        DeployerFakePlayer fake = deployer.getPlayer();
        if (fake == null)
            throw new IllegalStateException("deployer contract: the probe deployer has no fake player");

        for (JsonElement e : doc.getAsJsonArray("blocks")) {
            JsonObject b = e.getAsJsonObject();
            BlockState state = state(b.get("state").getAsString());
            level.setBlock(o, state, 3);
            if (b.has("hardness"))
                expect("hardness of " + b.get("state").getAsString(), b.get("hardness").getAsFloat(),
                        state.getDestroySpeed(level, o), 1e-6);
        }

        // `DeployerBlockEntity.start`'s ray, 3/2 to 5/2 ahead of the deployer's centre: how far it
        // runs before the state two ahead stops it, facing down, up and east. The same state one
        // ahead, with air two ahead, never does (1, a miss).
        BlockPos ray = o.north(8);
        for (JsonElement e : doc.getAsJsonArray("start_rays")) {
            JsonObject r = e.getAsJsonObject();
            BlockState state = state(r.get("state").getAsString());
            for (String where : List.of("ahead", "before"))
                for (Direction facing : List.of(Direction.DOWN, Direction.UP, Direction.EAST)) {
                    for (int i = 1; i <= 2; i++)
                        level.setBlock(ray.relative(facing, i), Blocks.AIR.defaultBlockState(), 3);
                    level.setBlock(ray.relative(facing, where.equals("ahead") ? 2 : 1), state, 3);
                    Vec3 normal = Vec3.atLowerCornerOf(facing.getNormal());
                    Vec3 from = Vec3.atCenterOf(ray).add(normal.scale(3 / 2f));
                    Vec3 to = Vec3.atCenterOf(ray).add(normal.scale(5 / 2f));
                    BlockHitResult hit = level.clip(new ClipContext(from, to, ClipContext.Block.OUTLINE,
                            ClipContext.Fluid.NONE, fake));
                    expect("start ray " + where + " " + facing.getSerializedName() + " at " + r.get("state").getAsString(),
                            where.equals("ahead") ? r.get(facing.getSerializedName()).getAsDouble() : 1.0,
                            hit.getLocation().subtract(from).length(), 1e-6);
                    for (int i = 1; i <= 2; i++)
                        level.setBlock(ray.relative(facing, i), Blocks.AIR.defaultBlockState(), 3);
                }
        }

        for (JsonElement e : doc.getAsJsonArray("dig")) {
            JsonObject d = e.getAsJsonObject();
            BlockState state = state(d.get("block").getAsString());
            String what = "a punch with " + d.get("held").getAsString() + " on " + d.get("block").getAsString();
            level.setBlock(o, state, 3);
            ItemStack tool = item(d.get("held").getAsString(), 1);
            fake.setItemInHand(InteractionHand.MAIN_HAND, tool);
            expect("tool speed of " + what, d.get("speed").getAsFloat(), tool.getDestroySpeed(state), 1e-6);
            boolean harvest = d.get("harvest").getAsBoolean();
            expect("harvest of " + what, harvest, state.canHarvestBlock(level, o, fake));
            List<String> drops = Block.getDrops(state, level, o, null, fake, tool).stream().map(DeployerAudit::of)
                    .sorted().toList();
            // A block its tool cannot harvest drops nothing (`tryHarvestBlock` returns first);
            // the loot table itself names no tool.
            if (harvest) expect("drops of " + what, stacks(d.getAsJsonArray("drops")), drops);
            else expect("drops of " + what + ", which it cannot harvest", List.of(), stacks(d.getAsJsonArray("drops")));
            // `getDestroyProgress`: the tool's speed over five (a fake player is never on the
            // ground), over the hardness, over 30 when it harvests and 100 when it does not.
            float hardness = state.getDestroySpeed(level, o);
            float expected = hardness == -1.0F ? 0.0F
                    : tool.getDestroySpeed(state) / 5.0F / hardness / (harvest ? 30.0F : 100.0F);
            expect("destroy progress of " + what, expected, state.getDestroyProgress(fake, level, o), 1e-6);
        }

        for (JsonObject i : items.values()) {
            String id = i.get("id").getAsString();
            if (i.has("turns"))
                turns(level, o, fake, states, i, id, !i.has("turns_from_below") || i.get("turns_from_below").getAsBoolean());
            if (!i.has("block")) continue;
            BlockState placed = state(i.get("block").getAsString());
            for (Direction face : List.of(Direction.UP, Direction.SOUTH)) {
                level.setBlock(o, Blocks.AIR.defaultBlockState(), 3);
                ItemStack stack = item(id, 3);
                fake.setItemInHand(InteractionHand.MAIN_HAND, stack);
                String what = id + " placed by a click " + face.getSerializedName();
                expect(what, InteractionResult.CONSUME, use(fake, stack, o, face));
                BlockState now = level.getBlockState(o);
                expect(what + " places", placed.getBlock(), now.getBlock());
                for (Property<?> p : placed.getProperties())
                    if (p.getName().equals("axis") && i.get("block").getAsString().startsWith("minecraft:"))
                        expect(what + " axis", face.getAxis(), now.getValue((Property<Direction.Axis>) p));
                expect(what + " uses one", 2, stack.getCount());
            }
            // BlockItemMixin: a fake player's block item never places against a block.
            level.setBlock(o, Blocks.STONE.defaultBlockState(), 3);
            ItemStack stack = item(id, 3);
            fake.setItemInHand(InteractionHand.MAIN_HAND, stack);
            expect(id + " against stone", InteractionResult.PASS, use(fake, stack, o, Direction.UP));
            expect(id + " against stone places nothing", Blocks.STONE, level.getBlockState(o).getBlock());
            expect(id + " against stone keeps the stack", 3, stack.getCount());
        }

        JsonObject recipes = doc.getAsJsonObject("recipes");
        JsonArray deploying = recipes.getAsJsonArray("deploying");
        JsonArray application = recipes.getAsJsonArray("application");
        JsonArray polishing = recipes.getAsJsonArray("polishing");
        List<JsonArray> groups = List.of(deploying, application, polishing);
        for (JsonArray group : groups)
            for (JsonElement e : group) {
                JsonObject r = e.getAsJsonObject();
                RecipeHolder<?> holder = level.getRecipeManager()
                        .byKey(ResourceLocation.parse(r.get("id").getAsString()))
                        .orElseThrow(() -> new IllegalStateException("deployer contract: no recipe " + r.get("id").getAsString()));
                if (!(holder.value() instanceof ProcessingRecipe<?, ?> recipe))
                    throw new IllegalStateException("deployer contract: not a processing recipe " + holder.id());
                List<Ingredient> ingredients = recipe.getIngredients();
                expect("processed items of " + holder.id(), ids(r.getAsJsonArray("target")),
                        items.keySet().stream().filter(t -> ingredients.getFirst().test(item(t, 1))).sorted().toList());
                expect("results of " + holder.id(), results(r.getAsJsonArray("results")),
                        recipe.getRollableResults().stream().map(DeployerAudit::of).sorted().toList());
                if (group != polishing) {
                    if (!(recipe instanceof ItemApplicationRecipe itemApplication))
                        throw new IllegalStateException("deployer contract: not an item application recipe " + holder.id());
                    expect("held items of " + holder.id(), ids(r.getAsJsonArray("tool")),
                            items.keySet().stream().filter(t -> ingredients.get(1).test(item(t, 1))).sorted().toList());
                    expect("keep held of " + holder.id(), r.get("keep_held").getAsBoolean(), itemApplication.shouldKeepHeldItem());
                }
            }
        List<String> tools = new ArrayList<>(items.keySet());
        tools.add(HAND);
        for (String target : items.keySet())
            for (String tool : tools) {
                fake.setItemInHand(InteractionHand.MAIN_HAND, tool.equals(HAND) ? ItemStack.EMPTY : item(tool, 1));
                RecipeHolder<?> found = deployer.getRecipe(item(target, 1));
                String want = null;
                if (tool.equals("create:sand_paper") || tool.equals("create:red_sand_paper"))
                    want = first(polishing, target, tool);
                if (want == null) want = first(deploying, target, tool);
                if (want == null) want = first(application, target, tool);
                expect("deployer recipe for " + target + " with " + tool, want,
                        found == null ? null : found.id().toString());
            }
        for (JsonObject target : items.values()) {
            if (!target.has("block")) continue;
            BlockState state = state(target.get("block").getAsString());
            for (String tool : items.keySet()) {
                ItemStack held = item(tool, 1);
                net.minecraft.world.item.crafting.RecipeType<net.minecraft.world.item.crafting.Recipe<net.neoforged.neoforge.items.wrapper.RecipeWrapper>> type =
                        AllRecipeTypes.ITEM_APPLICATION.getType();
                RecipeHolder<?> found = level.getRecipeManager().getAllRecipesFor(type).stream()
                        .filter(h -> {
                            ManualApplicationRecipe recipe = (ManualApplicationRecipe) h.value();
                            return recipe.testBlock(state) && recipe.getRequiredHeldItem().test(held);
                        })
                        .findFirst().orElse(null);
                String id = target.get("id").getAsString();
                String want = application.asList().stream().map(JsonElement::getAsJsonObject)
                        .filter(r -> contains(r.getAsJsonArray("target"), id) && contains(r.getAsJsonArray("tool"), tool))
                        .map(r -> r.get("id").getAsString()).findFirst().orElse(null);
                expect("in-world application of " + tool + " on " + id, want,
                        found == null ? null : found.id().toString());
            }
        }
        fake.setItemInHand(InteractionHand.MAIN_HAND, ItemStack.EMPTY);
        fake.discard();
        if (!PROBLEMS.isEmpty())
            throw new IllegalStateException("deployer contract:\n" + String.join("\n", PROBLEMS));
    }

    /**
     * An axe, a shovel or a hoe turns the block it is used on, from the top or a side; the tool
     * wears. `fromBelow`: whether a click on the block's bottom does too (a shovel's does not).
     */
    static void turns(ServerLevel level, BlockPos o, DeployerFakePlayer fake, List<String> states, JsonObject i,
            String id, boolean fromBelow) {
        for (var entry : i.getAsJsonObject("turns").entrySet()) {
            String to = entry.getValue().getAsString();
            for (String from : states.stream().filter(s -> name(s).equals(entry.getKey())).toList())
                for (Direction face : List.of(Direction.UP, Direction.NORTH, Direction.DOWN)) {
                    level.setBlock(o, state(from), 3);
                    ItemStack tool = item(id, 1);
                    fake.setItemInHand(InteractionHand.MAIN_HAND, tool);
                    InteractionResult result = use(fake, tool, o, face);
                    String what = id + " on " + from + " clicked " + face.getSerializedName();
                    if (face == Direction.DOWN && !fromBelow) {
                        expect(what, InteractionResult.PASS, result);
                        expect(what + " leaves the block", state(from), level.getBlockState(o));
                        expect(what + " keeps the tool", 0, tool.getDamageValue());
                    } else {
                        expect(what, InteractionResult.CONSUME, result);
                        expect(what + " turns the block to", name(to),
                                key(level.getBlockState(o).getBlock().asItem()));
                        expect(what + " wears the tool", 1, tool.getDamageValue());
                    }
                }
        }
    }

    static boolean contains(JsonArray list, String id) {
        return list.asList().stream().anyMatch(t -> t.getAsString().equals(id));
    }

    static String first(JsonArray group, String target, String tool) {
        for (JsonElement e : group) {
            JsonObject r = e.getAsJsonObject();
            if (contains(r.getAsJsonArray("target"), target) && contains(r.getAsJsonArray("tool"), tool))
                return r.get("id").getAsString();
        }
        return null;
    }

    static InteractionResult use(DeployerFakePlayer fake, ItemStack stack, BlockPos pos, Direction face) {
        Vec3 hit = Vec3.atCenterOf(pos).add(Vec3.atLowerCornerOf(face.getNormal()).scale(0.5));
        return stack.useOn(new UseOnContext(fake, InteractionHand.MAIN_HAND, new BlockHitResult(hit, face, pos, false)));
    }

    static String name(String state) {
        return state.contains("[") ? state.substring(0, state.indexOf('[')) : state;
    }

    static BlockState state(String state) {
        try {
            return BlockStateParser.parseForBlock(BuiltInRegistries.BLOCK.asLookup(), state, false).blockState();
        } catch (Exception ex) {
            throw new IllegalStateException("deployer contract: bad state " + state, ex);
        }
    }

    static ItemStack item(String id, int count) {
        return new ItemStack(BuiltInRegistries.ITEM.get(ResourceLocation.parse(id)), count);
    }

    static String key(Item item) {
        return BuiltInRegistries.ITEM.getKey(item).toString();
    }

    static String of(ItemStack s) {
        return key(s.getItem()) + "x" + s.getCount();
    }

    static String of(com.simibubi.create.content.processing.recipe.ProcessingOutput out) {
        return of(out.getStack()) + "@" + out.getChance();
    }

    static String of(JsonObject o) {
        return o.get("item").getAsString() + "x" + o.get("count").getAsInt() + "@" + o.get("chance").getAsFloat();
    }

    static List<String> ids(JsonArray list) {
        return list.asList().stream().map(JsonElement::getAsString).sorted().toList();
    }

    static List<String> stacks(JsonArray list) {
        return list.asList().stream()
                .map(e -> e.getAsJsonObject().get("item").getAsString() + "x" + e.getAsJsonObject().get("count").getAsInt())
                .sorted().toList();
    }

    static List<String> results(JsonArray list) {
        return list.asList().stream().map(e -> of(e.getAsJsonObject())).sorted().toList();
    }

    static String text(JsonObject o, String k) {
        return o.has(k) ? o.get(k).getAsString() : null;
    }

    /** Every mismatch found so far: reported together at the end of the audit. */
    static final List<String> PROBLEMS = new ArrayList<>();

    static void expect(String what, Object want, Object got) {
        if (want == null ? got != null : !want.equals(got)) problem(what, want, got);
    }

    static void expect(String what, float want, float got, double tolerance) {
        if (Math.abs(want - got) > tolerance) problem(what, want, got);
    }

    static void expect(String what, double want, double got, double tolerance) {
        if (Math.abs(want - got) > tolerance) problem(what, want, got);
    }

    static void problem(String what, Object want, Object got) {
        if (PROBLEMS.size() < 80) PROBLEMS.add(what + ": manifest " + want + ", server " + got);
    }
}
