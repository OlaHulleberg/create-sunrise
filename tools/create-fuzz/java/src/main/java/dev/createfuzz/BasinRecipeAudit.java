package dev.createfuzz;

import com.google.gson.*;
import com.simibubi.create.AllRecipeTypes;
import com.simibubi.create.content.kinetics.crafter.MechanicalCraftingRecipe;
import com.simibubi.create.content.kinetics.press.MechanicalPressBlockEntity;
import com.simibubi.create.content.processing.basin.BasinRecipe;
import com.simibubi.create.infrastructure.config.AllConfigs;
import net.minecraft.core.HolderLookup;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.crafting.CraftingRecipe;
import net.minecraft.world.item.crafting.Ingredient;
import net.minecraft.world.item.crafting.Recipe;
import net.minecraft.world.item.crafting.RecipeHolder;
import net.minecraft.world.item.crafting.ShapedRecipe;
import net.neoforged.neoforge.fluids.FluidStack;
import net.neoforged.neoforge.fluids.crafting.SizedFluidIngredient;
import java.util.*;
import java.util.function.Predicate;

/** Checks the basin families' recipes (`FUZZ_BASIN_MANIFEST`) against the loaded data: each entry as Create
 * reads it, and that no recipe of a family's kinds is missing whose ingredients its basin can hold. A basin
 * holds its family's inputs and fluids, what its recipes make and the containers these and what they use
 * up leave. The mixer's `PotionMixingRecipes` take a potion, which no family holds. */
final class BasinRecipeAudit {
 /** The fluids a family's players empty into its basin. */
 static final Map<String,List<String>> FLUIDS=Map.of("mixing",List.of("minecraft:water","minecraft:milk"),
  "compacting",List.of("create:chocolate","create:honey"));

 static ItemStack item(String id) {return new ItemStack(BuiltInRegistries.ITEM.get(ResourceLocation.parse(id)));}
 static FluidStack fluid(String id,int amount) {return new FluidStack(BuiltInRegistries.FLUID.get(ResourceLocation.parse(id)),amount);}
 static String id(ItemStack s) {return BuiltInRegistries.ITEM.getKey(s.getItem()).toString();}
 static JsonArray array(JsonObject o,String key) {return o.has(key)?o.getAsJsonArray(key):new JsonArray();}
 static void fail(String why) {throw new IllegalStateException("basin recipe contract: "+why);}

 /** `MechanicalMixerBlockEntity.matchStaticFilters`. */
 static boolean mixing(RecipeHolder<?> h) {
  Recipe<?> r=h.value();
  return r.getType()==AllRecipeTypes.MIXING.getType()
   ||r instanceof CraftingRecipe&&!(r instanceof ShapedRecipe)&&AllConfigs.server().recipes.allowShapelessInMixer.get()
    &&r.getIngredients().size()>1&&!MechanicalPressBlockEntity.canCompress(r)&&!AllRecipeTypes.shouldIgnoreInAutomation(h);
 }
 /** `MechanicalPressBlockEntity.matchStaticFilters`. */
 static boolean compacting(RecipeHolder<?> h) {
  Recipe<?> r=h.value();
  return r.getType()==AllRecipeTypes.COMPACTING.getType()
   ||r instanceof CraftingRecipe&&!(r instanceof MechanicalCraftingRecipe)&&MechanicalPressBlockEntity.canCompress(r)
    &&!AllRecipeTypes.shouldIgnoreInAutomation(h);
 }
 /** Whether each ingredient of `r` takes one of these items, or fluids. */
 static boolean takes(Recipe<?> r,Set<String> items,Set<String> fluids) {
  for(Ingredient in:r.getIngredients()) if(items.stream().noneMatch(i->in.test(item(i)))) return false;
  if(r instanceof BasinRecipe b) for(SizedFluidIngredient in:b.getFluidIngredients())
   if(fluids.stream().noneMatch(f->in.test(fluid(f,in.amount())))) return false;
  return true;
 }
 /** The contract lists for `in` exactly the items of the basin it takes. */
 static void ingredient(Ingredient in,Set<String> items,JsonElement want,String where) {
  var names=new TreeSet<String>();for(var w:want.getAsJsonArray()) names.add(w.getAsString());
  for(String n:names) if(!in.test(item(n))) fail(where+" does not take "+n);
  var takes=new TreeSet<String>();for(String i:items) if(in.test(item(i))) takes.add(i);
  if(!takes.equals(names)) fail(where+" takes "+takes+", the contract "+names);
 }
 static void itemResult(ItemStack out,JsonElement want,String where) {
  var o=want.getAsJsonObject();
  if(!id(out).equals(o.get("item").getAsString())||out.getCount()!=o.get("count").getAsInt()) fail(where);
 }
 static void fluidResult(FluidStack out,JsonElement want,String where) {
  var o=want.getAsJsonArray();
  if(!FluidIds.of(out).equals(o.get(0).getAsString())||out.getAmount()!=o.get(1).getAsInt()) fail(where);
 }
 static void check(ServerLevel level) {
  JsonObject doc;
  try {doc=JsonParser.parseString(java.nio.file.Files.readString(java.nio.file.Path.of(System.getenv("FUZZ_BASIN_MANIFEST")))).getAsJsonObject();}
  catch(java.io.IOException e) {throw new IllegalStateException(e);}
  HolderLookup.Provider registries=level.registryAccess();
  var recipes=new ArrayList<RecipeHolder<?>>();for(var h:level.getRecipeManager().getRecipes()) recipes.add(h);
  var squares=new TreeMap<String,JsonObject>();
  for(var e:doc.getAsJsonArray("squares")) squares.put(e.getAsJsonObject().get("id").getAsString(),e.getAsJsonObject());
  for(String id:squares.keySet()) if(recipes.stream().noneMatch(h->h.id().toString().equals(id))) fail(id+" is not loaded");
  for(String family:List.of("mixing","compacting")) {
   boolean press=family.equals("compacting");
   Predicate<RecipeHolder<?>> kind=press?BasinRecipeAudit::compacting:BasinRecipeAudit::mixing;
   var listed=new TreeMap<String,JsonObject>();
   for(var e:doc.getAsJsonArray(family)) listed.put(e.getAsJsonObject().get("id").getAsString(),e.getAsJsonObject());
   if(!press) for(var e:doc.getAsJsonArray("shapeless")) listed.put(e.getAsJsonObject().get("id").getAsString(),e.getAsJsonObject());
   for(String id:listed.keySet()) if(recipes.stream().noneMatch(h->h.id().toString().equals(id))) fail(id+" is not loaded");
   var items=new TreeSet<String>();for(var e:doc.getAsJsonObject("inputs").getAsJsonArray(family)) items.add(e.getAsString());
   var fluids=new TreeSet<String>(FLUIDS.get(family));
   for(boolean more=true;more;) {
    more=false;
    for(var h:recipes) {
     Recipe<?> r=h.value();
     if(!kind.test(h)||!takes(r,items,fluids)) continue;
     var stacks=new ArrayList<ItemStack>();var poured=new ArrayList<FluidStack>();
     if(r instanceof BasinRecipe b) {for(var o:b.getRollableResults()) stacks.add(o.getStack());poured.addAll(b.getFluidResults());}
     else if(r instanceof CraftingRecipe c) stacks.add(c.getResultItem(registries));
     for(var in:r.getIngredients()) for(String i:items) if(in.test(item(i))) stacks.add(item(i));
     for(var s:stacks) {
      if(items.add(id(s))) more=true;
      if(!s.hasCraftingRemainingItem()) continue;
      var left=s.getCraftingRemainingItem();
      if(!left.isEmpty()&&items.add(id(left))) more=true;
     }
     for(var f:poured) if(fluids.add(FluidIds.of(f))) more=true;
    }
   }
   for(var h:recipes) {
    String id=h.id().toString();Recipe<?> r=h.value();
    var entry=listed.get(id);var square=press?squares.get(id):null;
    if(entry==null&&square==null) {
     if(kind.test(h)&&takes(r,items,fluids)) fail(family+" recipe not in the contract: "+id);
     continue;
    }
    if(!kind.test(h)||!takes(r,items,fluids)) fail(id+" is in the "+family+" contract but not one its basin can do");
    if(r instanceof BasinRecipe b&&entry!=null) {
     var ins=b.getIngredients();var wants=array(entry,"ingredients");
     if(ins.size()!=wants.size()) fail(id);
     for(int i=0;i<ins.size();i++) ingredient(ins.get(i),items,wants.get(i),id+" "+i);
     var fluidIns=b.getFluidIngredients();var fluidWants=array(entry,"fluids");
     if(fluidIns.size()!=fluidWants.size()) fail(id+": fluids");
     var used=new boolean[fluidIns.size()];
     for(var w:fluidWants) {
      var a=w.getAsJsonArray();var stack=fluid(a.get(0).getAsString(),a.get(1).getAsInt());boolean found=false;
      for(int i=0;i<fluidIns.size();i++) if(!used[i]&&fluidIns.get(i).amount()==stack.getAmount()&&fluidIns.get(i).test(stack)) {used[i]=true;found=true;break;}
      if(!found) fail(id+": no fluid ingredient takes "+w);
     }
     var outs=b.getRollableResults();var outWants=array(entry,"results");
     if(outs.size()!=outWants.size()) fail(id+": results");
     for(int i=0;i<outs.size();i++) {
      itemResult(outs.get(i).getStack(),outWants.get(i),id+": result "+i);
      if(outs.get(i).getChance()!=outWants.get(i).getAsJsonObject().get("chance").getAsFloat()) fail(id+": result "+i);
     }
     var pours=b.getFluidResults();var pourWants=array(entry,"fluid_results");
     if(pours.size()!=pourWants.size()) fail(id+": fluid results");
     for(int i=0;i<pours.size();i++) fluidResult(pours.get(i),pourWants.get(i),id+": fluid result "+i);
     if(!b.getRequiredHeat().name().toLowerCase(Locale.ROOT).equals(entry.has("heat")?entry.get("heat").getAsString():"none")
      ||b.getProcessingDuration()!=0) fail(id+": heat or time");
    } else if(r instanceof CraftingRecipe c&&entry!=null) {
     var ins=c.getIngredients();var wants=array(entry,"ingredients");
     if(ins.size()!=wants.size()) fail(id);
     for(int i=0;i<ins.size();i++) ingredient(ins.get(i),items,wants.get(i),id+" "+i);
     var outWants=entry.getAsJsonArray("results");
     if(outWants.size()!=1) fail(id+": results");
     itemResult(c.getResultItem(registries),outWants.get(0),id+": result");
    } else if(r instanceof CraftingRecipe c) {
     int size=square.get("size").getAsInt();String one=square.get("item").getAsString();
     var out=c.getResultItem(registries);
     if(size*size!=c.getIngredients().size()) fail(id+": square size");
     for(var in:c.getIngredients()) {
      if(!in.test(item(one))) fail(id+": square does not take "+one);
      // Each item a square takes has its own entry.
      for(String held:items) if(in.test(item(held))&&squares.values().stream().noneMatch(s->s.get("id").getAsString().equals(id)&&s.get("item").getAsString().equals(held)))
       fail(id+": square takes "+held+", which has no entry");
     }
     if(!id(out).equals(square.get("result").getAsString())||out.getCount()!=square.get("count").getAsInt()) fail(id+": square result");
    } else fail(id+" is neither a basin recipe nor a crafting recipe");
   }
  }
 }
}
