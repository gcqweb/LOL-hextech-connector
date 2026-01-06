# 选择英雄
@retry()
async def selectChampion(self, actionsId, championId, completed=None):
        data = {
            "championId": championId,
            'type': 'pick',
        }

        if completed:
            data['completed'] = True

        res = await self.__patch(
            f"/lol-champ-select/v1/session/actions/{actionsId}", data=data)

        return await res.read()

# 禁用英雄
@retry()
async def banChampion(self, actionsId, championId, completed=None):
        data = {
            "championId": championId,
            'type': 'ban',
        }

        if completed:
            data['completed'] = completed

        res = await self.__patch(
            f"/lol-champ-select/v1/session/actions/{actionsId}", data=data)

        return await res.read()


async def autoComplete(data, selection: ChampionSelection):
    """
    超时自动选定（当前选中英雄）
    """
    isAutoCompleted = cfg.get(cfg.enableAutoSelectTimeoutCompleted)
    if not isAutoCompleted or selection.isChampionPickedCompleted:
        return

    if not (localPlayerCellId := data.get('localPlayerCellId', None)):
        return

    for actionGroup in reversed(data['actions']):
        for action in actionGroup:
            if action['actorCellId'] != localPlayerCellId:
                continue

            if action['type'] != 'pick':
                continue

            if not action['isInProgress']:
                return False

            if action['completed']:
                selection.isChampionPickedCompleted = True
                return False

            break

    selection.isChampionPickedCompleted = True

    sleepTime = int(data['timer']['adjustedTimeLeftInPhase'] / 1000) - 4
    await asyncio.sleep(sleepTime)

    data = await connector.getChampSelectSession()

    if not data:
        return

    # 双方选过的英雄
    cantSelect = []

    # 双方 ban 掉的英雄
    bans = itertools.chain(data["bans"]['myTeamBans'],
                           data["bans"]['theirTeamBans'])

    championIntent = 0
    for actionGroup in data['actions']:
        for action in actionGroup:
            if (action['type'] == 'pick' and action['completed']
                    and action['actorCellId'] != localPlayerCellId):
                cantSelect.append(action['championId'])

            if action['actorCellId'] != localPlayerCellId:
                continue

            if action['type'] != 'pick':
                continue

            if action['completed']:
                return

            # 现在亮着的英雄
            championIntent = action['championId']
            actionId = action['id']

    if not championIntent:
        return

    cantSelect.extend(bans)

    if championIntent not in cantSelect:
        await connector.selectChampion(actionId, championIntent, True)
        return True

    pos = next(filter(lambda x: x['cellId'] ==
               localPlayerCellId, data['myTeam']), None)
    pos = pos.get('assignedPosition')

    if pos == 'top':
        candidates = deepcopy(cfg.get(cfg.autoSelectChampionTop))
    elif pos == 'jungle':
        candidates = deepcopy(cfg.get(cfg.autoSelectChampionJug))
    elif pos == 'middle':
        candidates = deepcopy(cfg.get(cfg.autoSelectChampionMid))
    elif pos == 'bottom':
        candidates = deepcopy(cfg.get(cfg.autoSelectChampionBot))
    elif pos == 'utility':
        candidates = deepcopy(cfg.get(cfg.autoSelectChampionSup))
    else:
        candidates = []

    candidates.extend(cfg.get(cfg.autoSelectChampion))

    candidates = [x for x in candidates if x not in cantSelect]

    if len(candidates) == 0:
        return

    await connector.selectChampion(actionId, candidates[0], True)

    return True


async def autoBan(data, selection: ChampionSelection):
    """
    自动禁用英雄
    """
    isAutoBan = cfg.get(cfg.enableAutoBanChampion)

    if not isAutoBan or selection.isChampionBanned:
        return

    localPlayerCellId = data['localPlayerCellId']
    for actionGroup in data['actions']:
        for action in actionGroup:
            if (action["actorCellId"] == localPlayerCellId
                    and action['type'] == 'ban'
                    and action["isInProgress"]):

                pos = next(
                    filter(lambda x: x['cellId'] == localPlayerCellId, data['myTeam']), None)
                pos = pos.get('assignedPosition')

                if pos == 'top':
                    candidates = deepcopy(cfg.get(cfg.autoBanChampionTop))
                elif pos == 'jungle':
                    candidates = deepcopy(cfg.get(cfg.autoBanChampionJug))
                elif pos == 'middle':
                    candidates = deepcopy(cfg.get(cfg.autoBanChampionMid))
                elif pos == 'bottom':
                    candidates = deepcopy(cfg.get(cfg.autoBanChampionBot))
                elif pos == 'utility':
                    candidates = deepcopy(cfg.get(cfg.autoBanChampionSup))
                else:
                    candidates = []

                candidates.extend(cfg.get(cfg.autoBanChampion))

                bans = itertools.chain(data["bans"]['myTeamBans'],
                                       data["bans"]['theirTeamBans'])
                candidates = [x for x in candidates if x not in bans]

                # 给队友一点预选的时间
                await asyncio.sleep(cfg.get(cfg.autoBanDelay))

                isFriendly = cfg.get(cfg.pretentBan)
                if isFriendly:
                    myTeam = (await connector.getChampSelectSession()).get("myTeam")

                    if not myTeam:
                        return

                    intents = [player["championPickIntent"]
                               for player in myTeam]
                    candidates = [x for x in candidates if x not in intents]

                if not candidates:
                    return

                championId = candidates[0]
                await connector.banChampion(action['id'], championId, True)
                selection.isChampionBanned = True

                return True